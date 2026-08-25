//! GTK-independent navigation state for Floe's Miller column view.

use std::{
    collections::VecDeque,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::SESSION_MAX_PATH_BYTES;

/// Maximum number of column locations retained for one Miller chain.
pub const MILLER_COLUMN_CAPACITY: usize = 16;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MillerColumnDepth(usize);

impl MillerColumnDepth {
    pub const fn get(self) -> usize {
        self.0
    }

    fn next(self) -> Result<Self, MillerStateError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(MillerStateError::DepthOverflow)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MillerColumn {
    depth: MillerColumnDepth,
    directory: PathBuf,
    selected_child: Option<PathBuf>,
}

impl MillerColumn {
    pub const fn depth(&self) -> MillerColumnDepth {
        self.depth
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn selected_child(&self) -> Option<&Path> {
        self.selected_child.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerChildKind {
    Directory,
    Leaf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerSelectionTransition {
    Selected {
        depth: MillerColumnDepth,
    },
    Descended {
        from: MillerColumnDepth,
        to: MillerColumnDepth,
        evicted: Option<MillerColumnDepth>,
    },
    ActivatedExisting {
        depth: MillerColumnDepth,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerReconcileTransition {
    Unchanged,
    Renamed { first_affected: MillerColumnDepth },
    SelectionCleared { depth: MillerColumnDepth },
    Truncated { first_removed: MillerColumnDepth },
    RootInvalidated { first_removed: MillerColumnDepth },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MillerColumnModel {
    columns: VecDeque<MillerColumn>,
    active_depth: Option<MillerColumnDepth>,
}

impl MillerColumnModel {
    pub fn new(root: PathBuf) -> Result<Self, MillerStateError> {
        validate_path(&root)?;
        Ok(Self {
            columns: VecDeque::from([MillerColumn {
                depth: MillerColumnDepth(0),
                directory: root,
                selected_child: None,
            }]),
            active_depth: Some(MillerColumnDepth(0)),
        })
    }

    pub fn reset(&mut self, root: PathBuf) -> Result<(), MillerStateError> {
        *self = Self::new(root)?;
        Ok(())
    }

    pub fn columns(&self) -> impl DoubleEndedIterator<Item = &MillerColumn> + ExactSizeIterator {
        self.columns.iter()
    }

    pub fn len(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    pub fn first_retained_depth(&self) -> Option<MillerColumnDepth> {
        self.columns.front().map(MillerColumn::depth)
    }

    pub fn last_retained_depth(&self) -> Option<MillerColumnDepth> {
        self.columns.back().map(MillerColumn::depth)
    }

    pub const fn active_depth(&self) -> Option<MillerColumnDepth> {
        self.active_depth
    }

    pub fn column(&self, depth: MillerColumnDepth) -> Result<&MillerColumn, MillerStateError> {
        let index = self.index_for(depth)?;
        Ok(&self.columns[index])
    }

    pub fn activate(&mut self, depth: MillerColumnDepth) -> Result<bool, MillerStateError> {
        self.index_for(depth)?;
        let changed = self.active_depth != Some(depth);
        self.active_depth = Some(depth);
        Ok(changed)
    }

    pub fn select_child(
        &mut self,
        depth: MillerColumnDepth,
        child: PathBuf,
        kind: MillerChildKind,
    ) -> Result<MillerSelectionTransition, MillerStateError> {
        validate_path(&child)?;
        let index = self.index_for(depth)?;
        let directory = self.columns[index].directory.clone();
        if child.parent() != Some(directory.as_path()) {
            return Err(MillerStateError::NotDirectChild { directory, child });
        }

        let already_selected = self.columns[index].selected_child.as_ref() == Some(&child);
        if already_selected && kind == MillerChildKind::Directory {
            let next_depth = depth.next()?;
            if self
                .columns
                .get(index + 1)
                .is_some_and(|column| column.depth == next_depth && column.directory == child)
            {
                self.active_depth = Some(next_depth);
                return Ok(MillerSelectionTransition::ActivatedExisting { depth: next_depth });
            }
        }

        self.columns.truncate(index + 1);
        self.columns[index].selected_child = Some(child.clone());
        self.active_depth = Some(depth);
        if kind == MillerChildKind::Leaf {
            return Ok(MillerSelectionTransition::Selected { depth });
        }

        let next_depth = depth.next()?;
        self.columns.push_back(MillerColumn {
            depth: next_depth,
            directory: child,
            selected_child: None,
        });
        self.active_depth = Some(next_depth);
        let evicted = if self.columns.len() > MILLER_COLUMN_CAPACITY {
            self.columns.pop_front().map(|column| column.depth)
        } else {
            None
        };
        Ok(MillerSelectionTransition::Descended {
            from: depth,
            to: next_depth,
            evicted,
        })
    }

    pub fn clear_selection(&mut self, depth: MillerColumnDepth) -> Result<bool, MillerStateError> {
        let index = self.index_for(depth)?;
        let changed =
            self.columns[index].selected_child.take().is_some() || self.columns.len() > index + 1;
        self.columns.truncate(index + 1);
        self.active_depth = Some(depth);
        Ok(changed)
    }

    pub fn rename_path(
        &mut self,
        old_path: &Path,
        new_path: PathBuf,
    ) -> Result<MillerReconcileTransition, MillerStateError> {
        validate_path(old_path)?;
        validate_path(&new_path)?;
        if old_path == new_path {
            return Ok(MillerReconcileTransition::Unchanged);
        }
        if old_path.parent() != new_path.parent() {
            return Err(MillerStateError::RenameChangesParent {
                old_path: old_path.to_path_buf(),
                new_path,
            });
        }

        let mut first_affected = None;
        for column in &mut self.columns {
            if let Some(remapped) = remap_prefix(&column.directory, old_path, &new_path) {
                first_affected.get_or_insert(column.depth);
                column.directory = remapped;
            }
            if let Some(selected) = column.selected_child.as_mut()
                && let Some(remapped) = remap_prefix(selected, old_path, &new_path)
            {
                first_affected.get_or_insert(column.depth);
                *selected = remapped;
            }
        }

        Ok(
            first_affected.map_or(MillerReconcileTransition::Unchanged, |first_affected| {
                MillerReconcileTransition::Renamed { first_affected }
            }),
        )
    }

    pub fn remove_path(
        &mut self,
        removed: &Path,
    ) -> Result<MillerReconcileTransition, MillerStateError> {
        validate_path(removed)?;
        let affected_column = self.columns.iter().position(|column| {
            column.directory == removed || column.directory.starts_with(removed)
        });
        if let Some(index) = affected_column {
            let first_removed = self.columns[index].depth;
            if index == 0 {
                self.columns.clear();
                self.active_depth = None;
                return Ok(MillerReconcileTransition::RootInvalidated { first_removed });
            }
            self.columns[index - 1].selected_child = None;
            self.columns.truncate(index);
            self.active_depth = self.columns.back().map(MillerColumn::depth);
            return Ok(MillerReconcileTransition::Truncated { first_removed });
        }

        if let Some(index) = self.columns.iter().position(|column| {
            column
                .selected_child
                .as_deref()
                .is_some_and(|selected| selected == removed || selected.starts_with(removed))
        }) {
            let depth = self.columns[index].depth;
            let had_descendant_column = self.columns.len() > index + 1;
            self.columns[index].selected_child = None;
            self.columns.truncate(index + 1);
            self.active_depth = Some(depth);
            return if had_descendant_column {
                Ok(MillerReconcileTransition::Truncated {
                    first_removed: depth.next()?,
                })
            } else {
                Ok(MillerReconcileTransition::SelectionCleared { depth })
            };
        }

        Ok(MillerReconcileTransition::Unchanged)
    }

    fn index_for(&self, depth: MillerColumnDepth) -> Result<usize, MillerStateError> {
        let Some(first) = self.first_retained_depth() else {
            return Err(MillerStateError::EmptyModel);
        };
        let Some(last) = self.last_retained_depth() else {
            return Err(MillerStateError::EmptyModel);
        };
        let Some(index) = depth.0.checked_sub(first.0) else {
            return Err(MillerStateError::DepthNotRetained {
                requested: depth,
                first,
                last,
            });
        };
        if index >= self.columns.len() || self.columns[index].depth != depth {
            return Err(MillerStateError::DepthNotRetained {
                requested: depth,
                first,
                last,
            });
        }
        Ok(index)
    }
}

fn validate_path(path: &Path) -> Result<(), MillerStateError> {
    if !path.is_absolute() || path.as_os_str().as_bytes().len() > SESSION_MAX_PATH_BYTES {
        return Err(MillerStateError::InvalidPath(path.to_path_buf()));
    }
    Ok(())
}

fn remap_prefix(path: &Path, old_prefix: &Path, new_prefix: &Path) -> Option<PathBuf> {
    if path == old_prefix {
        return Some(new_prefix.to_path_buf());
    }
    path.strip_prefix(old_prefix)
        .ok()
        .map(|suffix| new_prefix.join(suffix))
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum MillerStateError {
    #[error("Miller paths must be absolute and bounded: {0:?}")]
    InvalidPath(PathBuf),
    #[error("the Miller model has no valid retained root; reset is required")]
    EmptyModel,
    #[error(
        "column depth {requested:?} is not retained; retained range is {first:?} through {last:?}"
    )]
    DepthNotRetained {
        requested: MillerColumnDepth,
        first: MillerColumnDepth,
        last: MillerColumnDepth,
    },
    #[error("selected path {child:?} is not a direct child of {directory:?}")]
    NotDirectChild { directory: PathBuf, child: PathBuf },
    #[error("Miller logical column depth overflow")]
    DepthOverflow,
    #[error("rename reconciliation must retain the same parent: {old_path:?} -> {new_path:?}")]
    RenameChangesParent {
        old_path: PathBuf,
        new_path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use super::*;

    fn depth(value: usize) -> MillerColumnDepth {
        MillerColumnDepth(value)
    }

    #[test]
    fn phase_8a_chain_tracks_direct_selection_and_reuses_existing_descent() {
        let mut model = MillerColumnModel::new(PathBuf::from("/projects")).expect("model");
        assert_eq!(
            model
                .select_child(
                    depth(0),
                    PathBuf::from("/projects/readme.md"),
                    MillerChildKind::Leaf,
                )
                .expect("leaf selection"),
            MillerSelectionTransition::Selected { depth: depth(0) }
        );
        assert_eq!(model.len(), 1);

        assert_eq!(
            model
                .select_child(
                    depth(0),
                    PathBuf::from("/projects/floe"),
                    MillerChildKind::Directory,
                )
                .expect("directory descent"),
            MillerSelectionTransition::Descended {
                from: depth(0),
                to: depth(1),
                evicted: None,
            }
        );
        assert_eq!(
            model.column(depth(0)).expect("parent").selected_child(),
            Some(Path::new("/projects/floe"))
        );
        assert_eq!(
            model.column(depth(1)).expect("child").directory(),
            Path::new("/projects/floe")
        );
        assert_eq!(
            model
                .select_child(
                    depth(0),
                    PathBuf::from("/projects/floe"),
                    MillerChildKind::Directory,
                )
                .expect("existing descent"),
            MillerSelectionTransition::ActivatedExisting { depth: depth(1) }
        );
        assert!(matches!(
            model.select_child(
                depth(0),
                PathBuf::from("/outside"),
                MillerChildKind::Directory,
            ),
            Err(MillerStateError::NotDirectChild { .. })
        ));
        assert!(matches!(
            MillerColumnModel::new(PathBuf::from("relative")),
            Err(MillerStateError::InvalidPath(_))
        ));
    }

    #[test]
    fn phase_8a_bounds_retains_stable_logical_depths_and_rejects_evicted_depths() {
        let mut model = MillerColumnModel::new(PathBuf::from("/root")).expect("model");
        let mut current = PathBuf::from("/root");
        for logical_depth in 0..20 {
            current.push(format!("d{logical_depth}"));
            model
                .select_child(
                    depth(logical_depth),
                    current.clone(),
                    MillerChildKind::Directory,
                )
                .expect("bounded descent");
        }
        assert_eq!(model.len(), MILLER_COLUMN_CAPACITY);
        assert_eq!(model.first_retained_depth(), Some(depth(5)));
        assert_eq!(model.last_retained_depth(), Some(depth(20)));
        assert_eq!(model.active_depth(), Some(depth(20)));
        assert!(matches!(
            model.column(depth(4)),
            Err(MillerStateError::DepthNotRetained {
                requested,
                first,
                last,
            }) if requested == depth(4) && first == depth(5) && last == depth(20)
        ));
    }

    #[test]
    fn phase_8a_non_utf8_identity_survives_selection_descent_and_rename() {
        let root = PathBuf::from("/tmp");
        let raw = root.join(OsString::from_vec(b"raw-\xff".to_vec()));
        let renamed = root.join(OsString::from_vec(b"renamed-\xfe".to_vec()));
        let mut model = MillerColumnModel::new(root).expect("model");
        model
            .select_child(depth(0), raw.clone(), MillerChildKind::Directory)
            .expect("raw descent");
        assert_eq!(
            model
                .rename_path(&raw, renamed.clone())
                .expect("raw rename"),
            MillerReconcileTransition::Renamed {
                first_affected: depth(0)
            }
        );
        assert_eq!(
            model.column(depth(0)).expect("parent").selected_child(),
            Some(renamed.as_path())
        );
        assert_eq!(
            model.column(depth(1)).expect("child").directory(),
            renamed.as_path()
        );
    }

    #[test]
    fn phase_8a_reconcile_rename_delete_and_root_invalidation_are_deterministic() {
        let mut model = MillerColumnModel::new(PathBuf::from("/a")).expect("model");
        model
            .select_child(depth(0), PathBuf::from("/a/b"), MillerChildKind::Directory)
            .expect("first descent");
        model
            .select_child(
                depth(1),
                PathBuf::from("/a/b/c"),
                MillerChildKind::Directory,
            )
            .expect("second descent");
        assert_eq!(
            model
                .rename_path(Path::new("/a/b"), PathBuf::from("/a/d"))
                .expect("same-parent rename"),
            MillerReconcileTransition::Renamed {
                first_affected: depth(0)
            }
        );
        assert_eq!(
            model
                .column(depth(2))
                .expect("renamed descendant")
                .directory(),
            Path::new("/a/d/c")
        );
        assert!(matches!(
            model.rename_path(Path::new("/a/d"), PathBuf::from("/moved/d")),
            Err(MillerStateError::RenameChangesParent { .. })
        ));
        assert_eq!(
            model
                .remove_path(Path::new("/a/d/c"))
                .expect("child delete"),
            MillerReconcileTransition::Truncated {
                first_removed: depth(2)
            }
        );
        assert_eq!(model.len(), 2);
        assert_eq!(
            model
                .column(depth(1))
                .expect("retained parent")
                .selected_child(),
            None
        );
        model
            .select_child(depth(1), PathBuf::from("/a/d/leaf"), MillerChildKind::Leaf)
            .expect("leaf selection");
        assert_eq!(
            model
                .remove_path(Path::new("/a/d/leaf"))
                .expect("leaf delete"),
            MillerReconcileTransition::SelectionCleared { depth: depth(1) }
        );
        assert_eq!(
            model.remove_path(Path::new("/a")).expect("root delete"),
            MillerReconcileTransition::RootInvalidated {
                first_removed: depth(0)
            }
        );
        assert!(model.is_empty());
        assert_eq!(model.active_depth(), None);
        assert!(matches!(
            model.activate(depth(0)),
            Err(MillerStateError::EmptyModel)
        ));
        model.reset(PathBuf::from("/replacement")).expect("reset");
        assert_eq!(model.active_depth(), Some(depth(0)));
    }
}
