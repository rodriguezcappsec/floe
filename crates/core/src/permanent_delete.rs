use std::{
    collections::HashSet,
    fs, io,
    os::unix::{ffi::OsStringExt, fs::MetadataExt},
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermanentDeleteRequest {
    targets: Vec<PathBuf>,
}

impl PermanentDeleteRequest {
    pub fn new(targets: Vec<PathBuf>) -> Result<Self, PermanentDeleteRequestError> {
        if targets.is_empty() {
            return Err(PermanentDeleteRequestError::Empty);
        }

        let mut seen = HashSet::with_capacity(targets.len());
        for target in &targets {
            validate_target(target)?;
            if !seen.insert(target.clone()) {
                return Err(PermanentDeleteRequestError::Duplicate(target.clone()));
            }
        }

        for (index, target) in targets.iter().enumerate() {
            for other in targets.iter().skip(index + 1) {
                if target.starts_with(other) || other.starts_with(target) {
                    return Err(PermanentDeleteRequestError::Nested {
                        first: target.clone(),
                        second: other.clone(),
                    });
                }
            }
        }

        Ok(Self { targets })
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }
}

fn validate_target(target: &Path) -> Result<(), PermanentDeleteRequestError> {
    if !target.is_absolute() {
        return Err(PermanentDeleteRequestError::Relative(target.to_path_buf()));
    }
    if target.file_name().is_none() {
        return Err(PermanentDeleteRequestError::ProtectedRoot(
            target.to_path_buf(),
        ));
    }
    if target.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    }) {
        return Err(PermanentDeleteRequestError::Unnormalized(
            target.to_path_buf(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PermanentDeleteRequestError {
    #[error("select at least one item to delete permanently")]
    Empty,
    #[error("permanent deletion requires an absolute path: {}", .0.display())]
    Relative(PathBuf),
    #[error("filesystem roots cannot be deleted permanently: {}", .0.display())]
    ProtectedRoot(PathBuf),
    #[error("path must not contain dot or parent components: {}", .0.display())]
    Unnormalized(PathBuf),
    #[error("the same permanent-delete target appears twice: {}", .0.display())]
    Duplicate(PathBuf),
    #[error(
        "nested permanent-delete targets are ambiguous: {} and {}",
        first.display(),
        second.display()
    )]
    Nested { first: PathBuf, second: PathBuf },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermanentDeleteProgress {
    completed: u64,
    total: u64,
}

impl PermanentDeleteProgress {
    pub const fn new(completed: u64, total: u64) -> Option<Self> {
        if total == 0 || completed > total {
            None
        } else {
            Some(Self { completed, total })
        }
    }

    pub const fn completed(self) -> u64 {
        self.completed
    }

    pub const fn total(self) -> u64 {
        self.total
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PermanentDeleteOutcome {
    removed: u64,
}

impl PermanentDeleteOutcome {
    pub const fn removed(self) -> u64 {
        self.removed
    }
}

#[derive(Debug, Error)]
pub enum PermanentDeleteError {
    #[error("permanent deletion was cancelled before any item was removed")]
    Cancelled,
    #[error("could not inspect mounted filesystems: {0}")]
    MountInfo(#[source] io::Error),
    #[error("refusing to delete a mounted filesystem boundary: {}", path.display())]
    MountedBoundary { path: PathBuf },
    #[error("permanent-delete preflight failed for {}: {message}", path.display())]
    Preflight { path: PathBuf, message: String },
    #[error("permanent-delete target changed before removal: {}", path.display())]
    SourceChanged { path: PathBuf },
    #[error("could not delete {} permanently: {message}", path.display())]
    Io { path: PathBuf, message: String },
    #[error(
        "permanent deletion stopped after removing {removed} of {total} planned items; failed at {}: {message}",
        path.display()
    )]
    Partial {
        removed: u64,
        total: u64,
        path: PathBuf,
        message: String,
    },
}

impl PermanentDeleteError {
    pub const fn removed(&self) -> u64 {
        match self {
            Self::Partial { removed, .. } => *removed,
            _ => 0,
        }
    }

    pub const fn total(&self) -> Option<u64> {
        match self {
            Self::Partial { total, .. } => Some(*total),
            _ => None,
        }
    }

    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlannedKind {
    Directory,
    Symlink,
    Other,
}

#[derive(Clone, Debug)]
struct PlannedEntry {
    path: PathBuf,
    device: u64,
    inode: u64,
    kind: PlannedKind,
}

#[derive(Debug)]
struct DeletePlan {
    entries: Vec<PlannedEntry>,
}

pub fn execute_permanent_delete(
    request: &PermanentDeleteRequest,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(PermanentDeleteProgress),
) -> Result<PermanentDeleteOutcome, PermanentDeleteError> {
    let mount_points = read_mount_points().map_err(PermanentDeleteError::MountInfo)?;
    execute_permanent_delete_with_mounts(request, &mount_points, cancelled, &mut on_progress)
}

fn execute_permanent_delete_with_mounts(
    request: &PermanentDeleteRequest,
    mount_points: &HashSet<PathBuf>,
    cancelled: impl Fn() -> bool,
    on_progress: &mut impl FnMut(PermanentDeleteProgress),
) -> Result<PermanentDeleteOutcome, PermanentDeleteError> {
    if cancelled() {
        return Err(PermanentDeleteError::Cancelled);
    }
    let plan = build_plan(request, mount_points, &cancelled)?;
    if cancelled() {
        return Err(PermanentDeleteError::Cancelled);
    }

    let total = u64::try_from(plan.entries.len()).unwrap_or(u64::MAX);
    let mut removed = 0_u64;
    for entry in plan.entries {
        if removed == 0 && cancelled() {
            return Err(PermanentDeleteError::Cancelled);
        }

        if let Err(error) = revalidate(&entry) {
            return Err(execution_error(removed, total, entry.path, error));
        }
        let result = match entry.kind {
            PlannedKind::Directory => fs::remove_dir(&entry.path),
            PlannedKind::Symlink | PlannedKind::Other => fs::remove_file(&entry.path),
        };
        if let Err(error) = result {
            return Err(execution_error(
                removed,
                total,
                entry.path,
                error.to_string(),
            ));
        }
        removed = removed.saturating_add(1);
        on_progress(PermanentDeleteProgress {
            completed: removed,
            total,
        });
    }

    Ok(PermanentDeleteOutcome { removed })
}

fn build_plan(
    request: &PermanentDeleteRequest,
    mount_points: &HashSet<PathBuf>,
    cancelled: &impl Fn() -> bool,
) -> Result<DeletePlan, PermanentDeleteError> {
    let mut entries = Vec::new();
    for target in request.targets() {
        if cancelled() {
            return Err(PermanentDeleteError::Cancelled);
        }
        let metadata = metadata_for_preflight(target)?;
        let resolved = resolved_for_mount_check(target)?;
        if mount_points.contains(&resolved) || selected_target_crosses_device(target, &metadata)? {
            return Err(PermanentDeleteError::MountedBoundary {
                path: target.clone(),
            });
        }
        plan_path(
            target,
            metadata.dev(),
            mount_points,
            cancelled,
            &mut entries,
        )?;
    }
    Ok(DeletePlan { entries })
}

fn selected_target_crosses_device(
    path: &Path,
    metadata: &fs::Metadata,
) -> Result<bool, PermanentDeleteError> {
    if metadata.file_type().is_symlink() {
        return Ok(false);
    }
    let parent = path
        .parent()
        .ok_or_else(|| PermanentDeleteError::MountedBoundary {
            path: path.to_path_buf(),
        })?;
    let parent_metadata =
        fs::metadata(parent).map_err(|error| PermanentDeleteError::Preflight {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    Ok(parent_metadata.dev() != metadata.dev())
}

fn plan_path(
    path: &Path,
    root_device: u64,
    mount_points: &HashSet<PathBuf>,
    cancelled: &impl Fn() -> bool,
    entries: &mut Vec<PlannedEntry>,
) -> Result<(), PermanentDeleteError> {
    if cancelled() {
        return Err(PermanentDeleteError::Cancelled);
    }
    let metadata = metadata_for_preflight(path)?;
    let kind = planned_kind(&metadata);
    if kind != PlannedKind::Symlink
        && (metadata.dev() != root_device
            || mount_points.contains(&resolved_for_mount_check(path)?))
    {
        return Err(PermanentDeleteError::MountedBoundary {
            path: path.to_path_buf(),
        });
    }

    if kind == PlannedKind::Directory {
        let mut children = fs::read_dir(path)
            .map_err(|error| PermanentDeleteError::Preflight {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?
            .map(|entry| {
                entry
                    .map(|entry| entry.path())
                    .map_err(|error| PermanentDeleteError::Preflight {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        children.sort_by(|left, right| left.as_os_str().cmp(right.as_os_str()));
        for child in children {
            plan_path(&child, root_device, mount_points, cancelled, entries)?;
        }
    }

    entries.push(PlannedEntry {
        path: path.to_path_buf(),
        device: metadata.dev(),
        inode: metadata.ino(),
        kind,
    });
    Ok(())
}

fn metadata_for_preflight(path: &Path) -> Result<fs::Metadata, PermanentDeleteError> {
    fs::symlink_metadata(path).map_err(|error| PermanentDeleteError::Preflight {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn resolved_for_mount_check(path: &Path) -> Result<PathBuf, PermanentDeleteError> {
    let parent = path
        .parent()
        .ok_or_else(|| PermanentDeleteError::MountedBoundary {
            path: path.to_path_buf(),
        })?;
    let name = path
        .file_name()
        .ok_or_else(|| PermanentDeleteError::MountedBoundary {
            path: path.to_path_buf(),
        })?;
    fs::canonicalize(parent)
        .map(|parent| parent.join(name))
        .map_err(|error| PermanentDeleteError::Preflight {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })
}

fn revalidate(entry: &PlannedEntry) -> Result<(), String> {
    let metadata = fs::symlink_metadata(&entry.path).map_err(|error| error.to_string())?;
    if metadata.dev() != entry.device
        || metadata.ino() != entry.inode
        || planned_kind(&metadata) != entry.kind
    {
        return Err("path identity changed after preflight".to_owned());
    }
    Ok(())
}

fn planned_kind(metadata: &fs::Metadata) -> PlannedKind {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        PlannedKind::Symlink
    } else if file_type.is_dir() {
        PlannedKind::Directory
    } else {
        PlannedKind::Other
    }
}

fn execution_error(
    removed: u64,
    total: u64,
    path: PathBuf,
    message: impl Into<String>,
) -> PermanentDeleteError {
    let message = message.into();
    if removed == 0 {
        PermanentDeleteError::Io { path, message }
    } else {
        PermanentDeleteError::Partial {
            removed,
            total,
            path,
            message,
        }
    }
}

fn read_mount_points() -> io::Result<HashSet<PathBuf>> {
    let bytes = fs::read("/proc/self/mountinfo")?;
    let mut mount_points = HashSet::new();
    for line in bytes.split(|byte| *byte == b'\n') {
        let Some(field) = line.split(|byte| *byte == b' ').nth(4) else {
            continue;
        };
        mount_points.insert(PathBuf::from(std::ffi::OsString::from_vec(
            decode_mount_field(field),
        )));
    }
    Ok(mount_points)
}

fn decode_mount_field(field: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(field.len());
    let mut index = 0;
    while index < field.len() {
        if field[index] == b'\\'
            && index + 3 < field.len()
            && field[index + 1..=index + 3]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            let value = (field[index + 1] - b'0') * 64
                + (field[index + 2] - b'0') * 8
                + (field[index + 3] - b'0');
            decoded.push(value);
            index += 4;
        } else {
            decoded.push(field[index]);
            index += 1;
        }
    }
    decoded
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
        sync::atomic::{AtomicBool, Ordering},
    };

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn phase_6m_request_preserves_raw_paths_and_rejects_unsafe_batches() {
        let raw = PathBuf::from("/tmp").join(OsString::from_vec(b"delete-\xff".to_vec()));
        let request = PermanentDeleteRequest::new(vec![raw.clone()]).expect("raw path should work");
        assert_eq!(request.targets(), &[raw]);

        assert!(matches!(
            PermanentDeleteRequest::new(Vec::new()),
            Err(PermanentDeleteRequestError::Empty)
        ));
        assert!(matches!(
            PermanentDeleteRequest::new(vec![PathBuf::from("relative")]),
            Err(PermanentDeleteRequestError::Relative(_))
        ));
        assert!(matches!(
            PermanentDeleteRequest::new(vec![PathBuf::from("/")]),
            Err(PermanentDeleteRequestError::ProtectedRoot(_))
        ));
        assert!(matches!(
            PermanentDeleteRequest::new(vec![PathBuf::from("/tmp/../item")]),
            Err(PermanentDeleteRequestError::Unnormalized(_))
        ));
        assert!(matches!(
            PermanentDeleteRequest::new(vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/a")]),
            Err(PermanentDeleteRequestError::Duplicate(_))
        ));
        assert!(matches!(
            PermanentDeleteRequest::new(vec![
                PathBuf::from("/tmp/folder"),
                PathBuf::from("/tmp/folder/child")
            ]),
            Err(PermanentDeleteRequestError::Nested { .. })
        ));
    }

    #[test]
    fn phase_6m_delete_removes_tree_and_symlink_without_following_target() {
        let root = tempdir().expect("temporary directory should be created");
        let target = root.path().join("outside.txt");
        let tree = root.path().join("tree");
        fs::create_dir(&tree).expect("tree should be created");
        fs::write(&target, b"keep").expect("target should be written");
        fs::write(tree.join("inside.txt"), b"remove").expect("inside should be written");
        symlink(&target, tree.join("link")).expect("symlink should be created");
        let request = PermanentDeleteRequest::new(vec![tree.clone()]).expect("request should work");
        let mut progress = Vec::new();
        let outcome = execute_permanent_delete_with_mounts(
            &request,
            &HashSet::new(),
            || false,
            &mut |value| progress.push(value),
        )
        .expect("tree deletion should succeed");

        assert_eq!(outcome.removed(), 3);
        assert_eq!(progress.last().map(|value| value.completed()), Some(3));
        assert!(!tree.exists());
        assert_eq!(
            fs::read(target).expect("outside target should remain"),
            b"keep"
        );
    }

    #[test]
    fn phase_6m_delete_preflights_every_target_and_refuses_mount_boundaries() {
        let root = tempdir().expect("temporary directory should be created");
        let safe = root.path().join("safe.txt");
        let mounted = root.path().join("mounted");
        fs::write(&safe, b"keep until full preflight").expect("safe file should be written");
        fs::create_dir(&mounted).expect("mounted fixture should be created");
        let request = PermanentDeleteRequest::new(vec![safe.clone(), mounted.clone()])
            .expect("request should work");
        let mounts = HashSet::from([fs::canonicalize(root.path())
            .expect("root should canonicalize")
            .join("mounted")]);
        assert!(matches!(
            execute_permanent_delete_with_mounts(&request, &mounts, || false, &mut |_| {}),
            Err(PermanentDeleteError::MountedBoundary { path }) if path == mounted
        ));
        assert!(safe.exists(), "preflight failure must mutate nothing");
        assert!(mounted.exists());
    }

    #[test]
    fn phase_6m_cancellation_before_commit_removes_nothing() {
        let root = tempdir().expect("temporary directory should be created");
        let target = root.path().join("item.txt");
        fs::write(&target, b"keep").expect("item should be written");
        let request =
            PermanentDeleteRequest::new(vec![target.clone()]).expect("request should work");
        assert!(matches!(
            execute_permanent_delete_with_mounts(&request, &HashSet::new(), || true, &mut |_| {}),
            Err(PermanentDeleteError::Cancelled)
        ));
        assert!(target.exists());
    }

    #[test]
    fn phase_6m_cancellation_after_commit_does_not_report_cancelled() {
        let root = tempdir().expect("temporary directory should be created");
        let first = root.path().join("first.txt");
        let second = root.path().join("second.txt");
        fs::write(&first, b"first").expect("first should be written");
        fs::write(&second, b"second").expect("second should be written");
        let request = PermanentDeleteRequest::new(vec![first.clone(), second.clone()])
            .expect("request should work");
        let cancel = AtomicBool::new(false);
        let outcome = execute_permanent_delete_with_mounts(
            &request,
            &HashSet::new(),
            || cancel.load(Ordering::Relaxed),
            &mut |_| cancel.store(true, Ordering::Relaxed),
        )
        .expect("post-commit cancellation should not interrupt deletion");
        assert_eq!(outcome.removed(), 2);
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn phase_6m_delete_revalidates_identity_and_reports_partial_failure() {
        let root = tempdir().expect("temporary directory should be created");
        let first = root.path().join("first.txt");
        let second = root.path().join("second.txt");
        fs::write(&first, b"first").expect("first should be written");
        fs::write(&second, b"second").expect("second should be written");
        let request = PermanentDeleteRequest::new(vec![first.clone(), second.clone()])
            .expect("request should work");
        let mut changed = false;
        let error =
            execute_permanent_delete_with_mounts(&request, &HashSet::new(), || false, &mut |_| {
                if !changed {
                    changed = true;
                    fs::remove_file(&second).expect("second should be replaced");
                    fs::write(&second, b"replacement").expect("replacement should be written");
                }
            })
            .expect_err("replacement after first removal should be partial failure");
        assert!(error.is_partial());
        assert_eq!(error.removed(), 1);
        assert_eq!(error.total(), Some(2));
        assert!(!first.exists());
        assert_eq!(
            fs::read(second).expect("replacement should remain"),
            b"replacement"
        );
    }

    #[test]
    fn phase_6m_delete_decodes_mountinfo_escapes_without_utf8_loss() {
        assert_eq!(
            decode_mount_field(b"/media/My\\040Drive"),
            b"/media/My Drive"
        );
        assert_eq!(decode_mount_field(b"/media/raw\\377"), b"/media/raw\xff");
    }
}
