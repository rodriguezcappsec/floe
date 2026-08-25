//! GTK-independent lifecycle for optional Miller final-column detail surfaces.
//!
//! Phase 8F only defines exact handoff state. Preview and Inspector providers
//! remain owned by Phases 9 and 10 respectively.

use std::{path::PathBuf, sync::Arc};

use floe_core::{DirectoryEntry, EntryKind};

pub const MILLER_DETAIL_SELECTION_CAPACITY: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MillerDetailSurface {
    Preview,
    Inspector,
}

impl MillerDetailSurface {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Preview => "Quick Preview",
            Self::Inspector => "Inspector",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MillerDetailTarget {
    generation: u64,
    surface: MillerDetailSurface,
    depth: usize,
    directory: PathBuf,
    paths: Vec<PathBuf>,
}

impl MillerDetailTarget {
    pub const fn surface(&self) -> MillerDetailSurface {
        self.surface
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum MillerDetailState {
    #[default]
    Hidden,
    Empty {
        surface: MillerDetailSurface,
    },
    Ready(MillerDetailTarget),
    Loading {
        target: MillerDetailTarget,
        request_generation: u64,
    },
    Provided {
        target: MillerDetailTarget,
        payload: crate::preview::PreviewPayload,
    },
    Unsupported {
        surface: MillerDetailSurface,
        reason: &'static str,
    },
    Failed {
        surface: MillerDetailSurface,
        message: String,
    },
}

impl MillerDetailState {
    pub const fn surface(&self) -> Option<MillerDetailSurface> {
        match self {
            Self::Hidden => None,
            Self::Empty { surface }
            | Self::Unsupported { surface, .. }
            | Self::Failed { surface, .. }
            | Self::Loading {
                target: MillerDetailTarget { surface, .. },
                ..
            }
            | Self::Provided {
                target: MillerDetailTarget { surface, .. },
                ..
            }
            | Self::Ready(MillerDetailTarget { surface, .. }) => Some(*surface),
        }
    }

    pub const fn is_visible(&self) -> bool {
        !matches!(self, Self::Hidden)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MillerDetailPresentation {
    pub title: &'static str,
    pub message: String,
    pub accessible_description: String,
}

impl From<&MillerDetailState> for MillerDetailPresentation {
    fn from(state: &MillerDetailState) -> Self {
        match state {
            MillerDetailState::Hidden => Self {
                title: "Details",
                message: "Detail surface hidden".to_owned(),
                accessible_description: "Miller detail surface hidden".to_owned(),
            },
            MillerDetailState::Empty { surface } => Self {
                title: surface.title(),
                message: "Select an item to prepare this detail surface.".to_owned(),
                accessible_description: format!(
                    "{} detail surface. No item selected.",
                    surface.title()
                ),
            },
            MillerDetailState::Ready(target) => Self {
                title: target.surface().title(),
                message: match target.surface() {
                    MillerDetailSurface::Preview => {
                        "Preview handoff ready. Content providers begin in Phase 9.".to_owned()
                    }
                    MillerDetailSurface::Inspector => format!(
                        "Inspector handoff ready for {} item{}. Metadata providers begin in Phase 10.",
                        target.paths().len(),
                        if target.paths().len() == 1 { "" } else { "s" }
                    ),
                },
                accessible_description: format!(
                    "{} provider handoff ready for {} selected item{}; content is not loaded yet.",
                    target.surface().title(),
                    target.paths().len(),
                    if target.paths().len() == 1 { "" } else { "s" }
                ),
            },
            MillerDetailState::Loading { target, .. } => Self {
                title: target.surface().title(),
                message: "Loading Preview…".to_owned(),
                accessible_description:
                    "Quick Preview is loading in a bounded background provider.".to_owned(),
            },
            MillerDetailState::Provided { target, payload } => Self {
                title: target.surface().title(),
                message: match &payload.content {
                    crate::preview::PreviewContent::Image {
                        width,
                        height,
                        first_frame_only,
                        ..
                    } => format!(
                        "Image preview, {width} by {height} pixels{}.",
                        if *first_frame_only {
                            ", first frame"
                        } else {
                            ""
                        }
                    ),
                    crate::preview::PreviewContent::Text { format, .. } => {
                        format!("Passive {format:?} source preview.")
                    }
                    crate::preview::PreviewContent::Document {
                        width,
                        height,
                        content_type,
                        first_page_only,
                        ..
                    } => format!(
                        "Document preview, {width} by {height} pixels, {content_type}{}.",
                        if *first_page_only {
                            ", first page rendition"
                        } else {
                            ""
                        }
                    ),
                    crate::preview::PreviewContent::Media {
                        content_type,
                        is_video,
                        poster,
                        ..
                    } => format!(
                        "{} preview, {content_type}; native playback controls{}.",
                        if *is_video { "Video" } else { "Audio" },
                        if poster.is_some() {
                            ", poster available"
                        } else {
                            ""
                        }
                    ),
                    crate::preview::PreviewContent::Font {
                        width,
                        height,
                        content_type,
                        ..
                    } => format!(
                        "Passive font specimen, {width} by {height} pixels, {content_type}."
                    ),
                    crate::preview::PreviewContent::Archive {
                        format,
                        entries,
                        truncated,
                        ..
                    } => format!(
                        "Read-only {format:?} listing, {} entr{}{}.",
                        entries.len(),
                        if entries.len() == 1 { "y" } else { "ies" },
                        if *truncated {
                            ", truncated by safety limits"
                        } else {
                            ""
                        }
                    ),
                    crate::preview::PreviewContent::None => "Preview is ready.".to_owned(),
                },
                accessible_description: format!(
                    "Quick Preview loaded by {} without executing file content.",
                    payload.provider_id
                ),
            },
            MillerDetailState::Unsupported { surface, reason } => Self {
                title: surface.title(),
                message: (*reason).to_owned(),
                accessible_description: format!("{} unavailable. {reason}", surface.title()),
            },
            MillerDetailState::Failed { surface, message } => Self {
                title: surface.title(),
                message: message.clone(),
                accessible_description: format!("{} failed. {message}", surface.title()),
            },
        }
    }
}

#[derive(Default)]
pub struct MillerDetailHooks {
    generation: u64,
    state: MillerDetailState,
}

impl MillerDetailHooks {
    pub fn state(&self) -> &MillerDetailState {
        &self.state
    }

    pub fn hide(&mut self) {
        self.state = MillerDetailState::Hidden;
    }

    pub fn begin_preview_loading(&mut self, request_generation: u64) -> bool {
        let MillerDetailState::Ready(target) = &self.state else {
            return false;
        };
        if target.surface != MillerDetailSurface::Preview || request_generation == 0 {
            return false;
        }
        self.state = MillerDetailState::Loading {
            target: target.clone(),
            request_generation,
        };
        true
    }

    pub fn finish_preview(
        &mut self,
        request_generation: u64,
        outcome: crate::preview::PreviewOutcome,
    ) -> bool {
        let MillerDetailState::Loading {
            target,
            request_generation: current,
        } = &self.state
        else {
            return false;
        };
        if *current != request_generation {
            return false;
        }
        let surface = target.surface;
        self.state = match outcome {
            crate::preview::PreviewOutcome::Ready(payload) => MillerDetailState::Provided {
                target: target.clone(),
                payload,
            },
            crate::preview::PreviewOutcome::Unsupported => MillerDetailState::Unsupported {
                surface,
                reason: "No Preview provider is available for this file type yet.",
            },
            crate::preview::PreviewOutcome::Cancelled => MillerDetailState::Failed {
                surface,
                message: "Preview was cancelled.".to_owned(),
            },
            crate::preview::PreviewOutcome::Failed(message) => {
                MillerDetailState::Failed { surface, message }
            }
        };
        true
    }

    pub fn toggle(
        &mut self,
        surface: MillerDetailSurface,
        depth: Option<usize>,
        directory: PathBuf,
        entries: &[Arc<DirectoryEntry>],
    ) {
        if self.state.surface() == Some(surface) {
            self.hide();
        } else {
            self.reconcile(surface, depth, directory, entries);
        }
    }

    pub fn refresh(
        &mut self,
        depth: Option<usize>,
        directory: PathBuf,
        entries: &[Arc<DirectoryEntry>],
    ) {
        if let Some(surface) = self.state.surface() {
            self.reconcile(surface, depth, directory, entries);
        }
    }

    fn reconcile(
        &mut self,
        surface: MillerDetailSurface,
        depth: Option<usize>,
        directory: PathBuf,
        entries: &[Arc<DirectoryEntry>],
    ) {
        let Some(depth) = depth else {
            self.state = MillerDetailState::Unsupported {
                surface,
                reason: "The active Miller column is no longer available.",
            };
            return;
        };
        if entries.is_empty() {
            self.state = MillerDetailState::Empty { surface };
            return;
        }
        if entries.len() > MILLER_DETAIL_SELECTION_CAPACITY {
            self.state = MillerDetailState::Unsupported {
                surface,
                reason: "The selection is too large for the bounded detail handoff.",
            };
            return;
        }
        if entries
            .iter()
            .any(|entry| entry.path().parent() != Some(directory.as_path()))
        {
            self.state = MillerDetailState::Unsupported {
                surface,
                reason: "The selected item no longer belongs to the active Miller column.",
            };
            return;
        }
        if surface == MillerDetailSurface::Preview {
            if entries.len() != 1 {
                self.state = MillerDetailState::Unsupported {
                    surface,
                    reason: "Quick Preview requires exactly one selected file.",
                };
                return;
            }
            if !matches!(
                entries[0].kind(),
                EntryKind::RegularFile
                    | EntryKind::SymbolicLink {
                        target_is_directory: false
                    }
            ) {
                self.state = MillerDetailState::Unsupported {
                    surface,
                    reason: "Quick Preview is not available for this filesystem entry.",
                };
                return;
            }
        }

        let paths = entries
            .iter()
            .map(|entry| entry.path().to_path_buf())
            .collect::<Vec<_>>();
        let current = match &self.state {
            MillerDetailState::Ready(target)
            | MillerDetailState::Loading { target, .. }
            | MillerDetailState::Provided { target, .. } => Some(target),
            _ => None,
        };
        if current.is_some_and(|current| {
            current.surface == surface
                && current.depth == depth
                && current.directory == directory
                && current.paths == paths
        }) {
            return;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        self.state = MillerDetailState::Ready(MillerDetailTarget {
            generation: self.generation,
            surface,
            depth,
            directory,
            paths,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::ffi::OsStringExt};

    use floe_core::enumerate_directory;
    use tempfile::tempdir;

    use super::*;

    fn fixture_entries() -> (tempfile::TempDir, Vec<Arc<DirectoryEntry>>, PathBuf) {
        let root = tempdir().expect("temporary root");
        let raw = root
            .path()
            .join(std::ffi::OsString::from_vec(b"preview-\xff.txt".to_vec()));
        fs::write(&raw, b"preview").expect("fixture");
        let entries = enumerate_directory(root.path())
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect();
        (root, entries, raw)
    }

    #[test]
    fn phase_8f_lifecycle_preserves_raw_targets_generations_and_stale_states() {
        let (root, entries, raw) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(2),
            root.path().to_path_buf(),
            &entries,
        );
        let MillerDetailState::Ready(first) = hooks.state() else {
            panic!("ready preview hook");
        };
        assert_eq!(first.generation, 1);
        assert_eq!(first.paths(), &[raw]);
        hooks.refresh(Some(2), root.path().to_path_buf(), &entries);
        let MillerDetailState::Ready(same) = hooks.state() else {
            panic!("stable preview hook");
        };
        assert_eq!(same.generation, 1);
        hooks.refresh(None, root.path().to_path_buf(), &entries);
        assert!(matches!(
            hooks.state(),
            MillerDetailState::Unsupported { .. }
        ));
        hooks.hide();
        assert_eq!(hooks.state(), &MillerDetailState::Hidden);
    }

    #[test]
    fn phase_8f_contract_keeps_preview_and_inspector_provider_boundaries_truthful() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Inspector,
            Some(0),
            root.path().to_path_buf(),
            &entries,
        );
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("Phase 10"));
        hooks.hide();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(0),
            root.path().to_path_buf(),
            &[],
        );
        assert!(matches!(hooks.state(), MillerDetailState::Empty { .. }));
        assert!(
            !MillerDetailPresentation::from(hooks.state())
                .message
                .contains("loaded")
        );
    }

    #[test]
    fn phase_8f_presentation_names_state_without_color_or_provider_claims() {
        let state = MillerDetailState::Unsupported {
            surface: MillerDetailSurface::Preview,
            reason: "Unsupported fixture",
        };
        let presentation = MillerDetailPresentation::from(&state);
        assert_eq!(presentation.title, "Quick Preview");
        assert!(presentation.accessible_description.contains("unavailable"));
        assert!(presentation.message.contains("Unsupported"));
    }

    #[test]
    fn phase_9a_lifecycle_applies_only_current_preview_worker_results() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_preview_loading(9));
        assert!(!hooks.finish_preview(8, crate::preview::PreviewOutcome::Unsupported));
        assert!(matches!(hooks.state(), MillerDetailState::Loading { .. }));
        assert!(hooks.finish_preview(9, crate::preview::PreviewOutcome::Unsupported));
        assert!(matches!(
            hooks.state(),
            MillerDetailState::Unsupported { .. }
        ));
        assert!(
            MillerDetailPresentation::from(hooks.state())
                .message
                .contains("No Preview provider")
        );
    }

    #[test]
    fn phase_9b_presentation_retains_only_matching_passive_payload() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_preview_loading(22));
        let payload = crate::preview::PreviewPayload {
            provider_id: "floe.text",
            kind: crate::preview::PreviewKind::Text,
            content: crate::preview::PreviewContent::Text {
                text: Arc::from("selectable inert source"),
                format: crate::preview::PreviewTextFormat::Plain,
            },
        };
        assert!(!hooks.finish_preview(21, crate::preview::PreviewOutcome::Ready(payload.clone())));
        assert!(matches!(hooks.state(), MillerDetailState::Loading { .. }));
        assert!(hooks.finish_preview(22, crate::preview::PreviewOutcome::Ready(payload)));
        assert!(matches!(
            hooks.state(),
            MillerDetailState::Provided {
                payload: crate::preview::PreviewPayload {
                    content: crate::preview::PreviewContent::Text { .. },
                    ..
                },
                ..
            }
        ));
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("Passive Plain source"));
        assert!(
            presentation
                .accessible_description
                .contains("without executing")
        );

        hooks.refresh(Some(1), root.path().to_path_buf(), &entries);
        assert!(matches!(hooks.state(), MillerDetailState::Provided { .. }));
        hooks.refresh(Some(1), root.path().to_path_buf(), &[]);
        assert!(matches!(hooks.state(), MillerDetailState::Empty { .. }));
    }

    #[test]
    fn phase_9c_document_presentation_labels_passive_first_page_and_rejects_stale() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_preview_loading(31));
        let payload = crate::preview::PreviewPayload {
            provider_id: "floe.document",
            kind: crate::preview::PreviewKind::Document,
            content: crate::preview::PreviewContent::Document {
                width: 2,
                height: 3,
                rowstride: 8,
                rgba: Arc::from(vec![0_u8; 24]),
                content_type: Arc::from("application/pdf"),
                first_page_only: true,
            },
        };
        assert!(!hooks.finish_preview(30, crate::preview::PreviewOutcome::Ready(payload.clone())));
        assert!(hooks.finish_preview(31, crate::preview::PreviewOutcome::Ready(payload)));
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("first page rendition"));
        assert!(presentation.message.contains("application/pdf"));
        assert!(
            presentation
                .accessible_description
                .contains("without executing")
        );
    }

    #[test]
    fn phase_9d_media_presentation_labels_controls_and_retires_payload_state() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_preview_loading(44));
        assert!(hooks.finish_preview(
            44,
            crate::preview::PreviewOutcome::Ready(crate::preview::PreviewPayload {
                provider_id: "floe.media",
                kind: crate::preview::PreviewKind::Media,
                content: crate::preview::PreviewContent::Media {
                    path: entries[0].path().to_path_buf(),
                    content_type: Arc::from("video/mp4"),
                    is_video: true,
                    poster: None,
                },
            })
        ));
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("native playback controls"));
        assert!(presentation.message.contains("video/mp4"));
        hooks.refresh(Some(1), root.path().to_path_buf(), &[]);
        assert!(matches!(hooks.state(), MillerDetailState::Empty { .. }));
        hooks.hide();
        assert_eq!(hooks.state(), &MillerDetailState::Hidden);
    }

    #[test]
    fn phase_9e_presentation_labels_passive_archive_and_retires_stale_payload() {
        let (root, entries, _) = fixture_entries();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Preview,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_preview_loading(55));
        let archive_entry = crate::preview::PreviewArchiveEntry {
            raw_name: Arc::from(b"../unsafe".as_slice()),
            display_name: Arc::from("../unsafe"),
            size: 3,
            is_directory: false,
            unsafe_path: true,
        };
        assert!(hooks.finish_preview(
            55,
            crate::preview::PreviewOutcome::Ready(crate::preview::PreviewPayload {
                provider_id: "floe.archive",
                kind: crate::preview::PreviewKind::Archive,
                content: crate::preview::PreviewContent::Archive {
                    format: crate::preview::PreviewArchiveFormat::Zip,
                    entries: Arc::from(vec![archive_entry]),
                    listing: Arc::from("[unsafe path] ../unsafe\tfile\t3 bytes\n"),
                    truncated: false,
                },
            })
        ));
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("Read-only Zip listing"));
        assert!(
            presentation
                .accessible_description
                .contains("without executing")
        );
        hooks.refresh(None, root.path().to_path_buf(), &entries);
        assert!(matches!(
            hooks.state(),
            MillerDetailState::Unsupported { .. }
        ));
    }
}
