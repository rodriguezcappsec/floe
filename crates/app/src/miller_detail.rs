//! GTK-independent lifecycle for optional Miller final-column detail surfaces.
//!
//! Phase 8F only defines exact handoff state. Preview and Inspector providers
//! remain owned by Phases 9 and 10 respectively.

use std::{path::PathBuf, sync::Arc};

use floe_core::{DirectoryEntry, EntryKind};

use crate::advanced_metadata::AdvancedMetadataState;

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

    pub fn directory(&self) -> &std::path::Path {
        &self.directory
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
    InspectorLoading {
        target: MillerDetailTarget,
        request_generation: u64,
    },
    Provided {
        target: MillerDetailTarget,
        payload: crate::preview::PreviewPayload,
    },
    Inspected {
        target: MillerDetailTarget,
        facts: crate::inspector::InspectorFacts,
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
            | Self::InspectorLoading {
                target: MillerDetailTarget { surface, .. },
                ..
            }
            | Self::Provided {
                target: MillerDetailTarget { surface, .. },
                ..
            }
            | Self::Inspected {
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
            MillerDetailState::InspectorLoading { target, .. } => Self {
                title: target.surface().title(),
                message: "Collecting selection facts…".to_owned(),
                accessible_description:
                    "Inspector is aggregating bounded selection facts in the background.".to_owned(),
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
            MillerDetailState::Inspected { target, facts } => Self {
                title: target.surface().title(),
                message: inspector_message(facts),
                accessible_description: format!(
                    "Read-only Inspector metadata summary for {} selected items. Folder sizes are immediate and non-recursive.",
                    facts.selection_count()
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

fn inspector_message(facts: &crate::inspector::InspectorFacts) -> String {
    use crate::inspector::{ImageDimensionFacts, SymlinkTargetStatus};

    let mut lines = vec![
        format!("{} selected", facts.selection_count()),
        format!(
            "{} files, {} folders, {} links, {} other",
            facts.regular_files, facts.directories, facts.symbolic_links, facts.other_entries
        ),
        format!(
            "{} known bytes; {} unknown sizes{}",
            facts.known_bytes,
            facts.unknown_sizes,
            if facts.bytes_overflowed {
                " (total overflowed)"
            } else {
                ""
            }
        ),
        format!("Common parent: {}", facts.common_parent.to_string_lossy()),
    ];

    let failures = facts
        .metadata
        .iter()
        .filter(|entry| entry.result.is_err())
        .count();
    if facts.selection_count() != 1 {
        lines.push(format!(
            "Metadata: {} loaded, {failures} unavailable",
            facts.metadata.len().saturating_sub(failures)
        ));
        let folders = facts
            .metadata
            .iter()
            .filter_map(|entry| entry.result.as_ref().ok()?.folder.as_ref())
            .collect::<Vec<_>>();
        if !folders.is_empty() {
            let children = folders.iter().fold(0usize, |sum, folder| {
                sum.saturating_add(folder.inspected_children)
            });
            let known_bytes = folders.iter().fold(0u64, |sum, folder| {
                sum.saturating_add(folder.known_immediate_bytes)
            });
            let limited = folders.iter().any(|folder| folder.truncated);
            lines.push(format!(
                "Selected folders: {children} immediate children, {known_bytes} known immediate bytes (non-recursive{})",
                if limited { ", limited" } else { "" }
            ));
        }
        return lines.join("\n");
    }

    let Some(entry) = facts.metadata.first() else {
        return lines.join("\n");
    };
    let details = match &entry.result {
        Ok(details) => details,
        Err(error) => {
            lines.push(format!("Metadata unavailable: {error}"));
            return lines.join("\n");
        }
    };
    lines.push(format!(
        "MIME type: {}",
        details.mime_type.as_deref().unwrap_or("Unknown")
    ));
    lines.push(format!(
        "Created: {} · Modified: {} · Accessed: {}",
        format_inspector_time(details.created),
        format_inspector_time(details.modified),
        format_inspector_time(details.accessed)
    ));
    if let (Some(uid), Some(gid), Some(mode)) =
        (details.unix_uid, details.unix_gid, details.unix_mode)
    {
        lines.push(format!(
            "Owner UID: {uid} · Group GID: {gid} · Mode: {:04o}",
            mode & 0o7777
        ));
    }
    match details.image_dimensions {
        ImageDimensionFacts::Dimensions(dimensions) => {
            lines.push(format!(
                "Image dimensions: {} × {} pixels",
                dimensions.width, dimensions.height
            ));
        }
        ImageDimensionFacts::Unavailable => {
            lines.push("Image dimensions: unavailable".to_owned());
        }
        ImageDimensionFacts::LimitExceeded => {
            lines.push("Image dimensions: withheld by safety limits".to_owned());
        }
        ImageDimensionFacts::NotImage => {}
    }
    match &details.advanced_metadata {
        AdvancedMetadataState::Present(metadata) => {
            if let Some(exif) = &metadata.exif {
                for field in exif.fields.iter() {
                    lines.push(format!("{}: {}", field.label, field.value));
                }
                if exif.values_truncated {
                    lines.push("EXIF text: truncated by safety limits".to_owned());
                }
            }
            if let Some(media) = &metadata.media {
                if let Some(duration) = media.duration {
                    lines.push(format!("Duration: {}", format_media_duration(duration)));
                }
                for (label, value) in [
                    ("Title", media.title.as_deref()),
                    ("Artist", media.artist.as_deref()),
                    ("Album", media.album.as_deref()),
                    ("Genre", media.genre.as_deref()),
                ] {
                    if let Some(value) = value {
                        lines.push(format!("{label}: {value}"));
                    }
                }
                if let Some(track) = media.track {
                    lines.push(match media.track_total {
                        Some(total) => format!("Track: {track} of {total}"),
                        None => format!("Track: {track}"),
                    });
                }
                if media.values_truncated {
                    lines.push("Media tag text: truncated by safety limits".to_owned());
                }
            }
        }
        AdvancedMetadataState::LimitExceeded => {
            lines.push("Advanced metadata: withheld by safety limits".to_owned());
        }
        AdvancedMetadataState::Malformed(error) => {
            lines.push(format!("Advanced metadata: malformed ({error})"));
        }
        AdvancedMetadataState::Unsupported | AdvancedMetadataState::NoMetadata => {}
    }
    if let Some(link) = &details.symlink {
        let status = match link.status {
            SymlinkTargetStatus::EntryPresent => "entry present",
            SymlinkTargetStatus::Missing => "missing",
            SymlinkTargetStatus::Inaccessible => "inaccessible",
        };
        lines.push(format!(
            "Stored link target: {} ({status})",
            link.stored_target.to_string_lossy()
        ));
    }
    if let Some(folder) = &details.folder {
        lines.push(format!(
            "Folder: {} immediate children; {} known immediate bytes (non-recursive{})",
            folder.inspected_children,
            folder.known_immediate_bytes,
            if folder.truncated { ", limited" } else { "" }
        ));
    }
    lines.push("Read-only metadata; no properties were changed.".to_owned());
    lines.join("\n")
}

fn format_inspector_time(value: Option<std::time::SystemTime>) -> String {
    use std::time::UNIX_EPOCH;

    let Some(value) = value else {
        return "Unknown".to_owned();
    };
    let seconds = match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_secs()).ok(),
        Err(error) => i64::try_from(error.duration().as_secs())
            .ok()
            .and_then(i64::checked_neg),
    };
    let Some(seconds) = seconds else {
        return "Unknown".to_owned();
    };
    glib::DateTime::from_unix_local(seconds)
        .ok()
        .and_then(|local| local.format("%x · %T").ok())
        .map_or_else(|| "Unknown".to_owned(), |formatted| formatted.to_string())
}

fn format_media_duration(duration: std::time::Duration) -> String {
    let total = duration.as_secs();
    let hours = total / 3_600;
    let minutes = (total % 3_600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
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

    pub fn begin_inspector_loading(&mut self, request_generation: u64) -> bool {
        let MillerDetailState::Ready(target) = &self.state else {
            return false;
        };
        if target.surface != MillerDetailSurface::Inspector || request_generation == 0 {
            return false;
        }
        self.state = MillerDetailState::InspectorLoading {
            target: target.clone(),
            request_generation,
        };
        true
    }

    pub fn finish_inspector(
        &mut self,
        request_generation: u64,
        result: Result<crate::inspector::InspectorFacts, crate::inspector::InspectorRequestError>,
    ) -> bool {
        let MillerDetailState::InspectorLoading {
            target,
            request_generation: current,
        } = &self.state
        else {
            return false;
        };
        if *current != request_generation {
            return false;
        }
        self.state = match result {
            Ok(facts) => MillerDetailState::Inspected {
                target: target.clone(),
                facts,
            },
            Err(error) => MillerDetailState::Failed {
                surface: MillerDetailSurface::Inspector,
                message: error.to_string(),
            },
        };
        true
    }

    pub fn finish_inspector_failure(
        &mut self,
        request_generation: u64,
        message: impl Into<String>,
    ) -> bool {
        let MillerDetailState::InspectorLoading {
            request_generation: current,
            ..
        } = &self.state
        else {
            return false;
        };
        if *current != request_generation {
            return false;
        }
        self.state = MillerDetailState::Failed {
            surface: MillerDetailSurface::Inspector,
            message: message.into(),
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
            | MillerDetailState::InspectorLoading { target, .. }
            | MillerDetailState::Provided { target, .. }
            | MillerDetailState::Inspected { target, .. } => Some(target),
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
            .collect::<Vec<_>>();
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
    fn phase_10a_inspector_lifecycle_rejects_stale_and_presents_read_only_multi_selection() {
        let (root, _, raw) = fixture_entries();
        let second = root.path().join("second.bin");
        fs::write(&second, b"12").expect("second fixture");
        let entries = floe_core::enumerate_directory(root.path())
            .expect("listing")
            .into_entries()
            .into_iter()
            .map(Arc::new)
            .collect::<Vec<_>>();
        let mut hooks = MillerDetailHooks::default();
        hooks.toggle(
            MillerDetailSurface::Inspector,
            Some(1),
            root.path().to_path_buf(),
            &entries,
        );
        assert!(hooks.begin_inspector_loading(71));
        let facts = crate::inspector::InspectorFacts {
            selection_paths: entries
                .iter()
                .map(|entry| entry.path().to_path_buf())
                .collect::<Vec<_>>()
                .into(),
            regular_files: 2,
            directories: 0,
            symbolic_links: 0,
            other_entries: 0,
            known_bytes: 9,
            unknown_sizes: 0,
            bytes_overflowed: false,
            common_parent: root.path().to_path_buf(),
            metadata: Arc::from([]),
        };
        assert!(!hooks.finish_inspector(70, Ok(facts.clone())));
        assert!(matches!(
            hooks.state(),
            MillerDetailState::InspectorLoading { .. }
        ));
        assert!(hooks.finish_inspector(71, Ok(facts)));
        let MillerDetailState::Inspected { target, .. } = hooks.state() else {
            panic!("Inspector facts ready");
        };
        assert_eq!(target.paths().len(), 2);
        assert!(target.paths().contains(&raw));
        let presentation = MillerDetailPresentation::from(hooks.state());
        assert!(presentation.message.contains("2 selected"));
        assert!(presentation.accessible_description.contains("Read-only"));
        hooks.refresh(Some(1), root.path().to_path_buf(), &entries);
        assert!(matches!(hooks.state(), MillerDetailState::Inspected { .. }));
        hooks.refresh(Some(1), root.path().to_path_buf(), &[]);
        assert!(matches!(hooks.state(), MillerDetailState::Empty { .. }));
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

    #[test]
    fn phase_10b_inspector_metadata_is_read_only_single_and_truthful_multi_selection() {
        use crate::inspector::{
            FolderAggregate, ImageDimensionFacts, ImageDimensions, InspectorEntryFacts,
            InspectorEntryResult, InspectorFacts,
        };

        let single = InspectorFacts {
            selection_paths: Arc::from([PathBuf::from("/tmp/photo.png")]),
            regular_files: 1,
            directories: 0,
            symbolic_links: 0,
            other_entries: 0,
            known_bytes: 42,
            unknown_sizes: 0,
            bytes_overflowed: false,
            common_parent: PathBuf::from("/tmp"),
            metadata: Arc::from([InspectorEntryResult {
                path: PathBuf::from("/tmp/photo.png"),
                result: Ok(InspectorEntryFacts {
                    path: PathBuf::from("/tmp/photo.png"),
                    mime_type: Some("image/png".to_owned()),
                    created: None,
                    modified: None,
                    accessed: None,
                    unix_uid: Some(1000),
                    unix_gid: Some(1001),
                    unix_mode: Some(0o100640),
                    symlink: None,
                    image_dimensions: ImageDimensionFacts::Dimensions(ImageDimensions {
                        width: 320,
                        height: 200,
                    }),
                    advanced_metadata: crate::advanced_metadata::AdvancedMetadataState::Present(
                        crate::advanced_metadata::AdvancedMetadata {
                            exif: Some(crate::advanced_metadata::ExifMetadata {
                                fields: Arc::from([crate::advanced_metadata::MetadataField {
                                    label: "Camera maker",
                                    value: "FloeCam".to_owned(),
                                }]),
                                values_truncated: false,
                            }),
                            media: None,
                        },
                    ),
                    folder: None,
                }),
            }]),
        };
        let single_message = inspector_message(&single);
        assert!(single_message.contains("MIME type: image/png"));
        assert!(single_message.contains("320 × 200"));
        assert!(single_message.contains("Camera maker: FloeCam"));
        assert!(single_message.contains("Owner UID: 1000"));
        assert!(single_message.contains("Mode: 0640"));
        assert!(single_message.contains("Read-only metadata"));

        let mut multi = single.clone();
        multi.selection_paths = Arc::from([
            PathBuf::from("/tmp/photo.png"),
            PathBuf::from("/tmp/folder"),
        ]);
        multi.directories = 1;
        multi.metadata = Arc::from([
            multi.metadata[0].clone(),
            InspectorEntryResult {
                path: PathBuf::from("/tmp/folder"),
                result: Ok(InspectorEntryFacts {
                    path: PathBuf::from("/tmp/folder"),
                    mime_type: Some("inode/directory".to_owned()),
                    created: None,
                    modified: None,
                    accessed: None,
                    unix_uid: Some(1000),
                    unix_gid: Some(1001),
                    unix_mode: Some(0o40750),
                    symlink: None,
                    image_dimensions: ImageDimensionFacts::NotImage,
                    advanced_metadata: crate::advanced_metadata::AdvancedMetadataState::Unsupported,
                    folder: Some(FolderAggregate {
                        inspected_children: 3,
                        regular_files: 2,
                        directories: 1,
                        known_immediate_bytes: 9,
                        ..FolderAggregate::default()
                    }),
                }),
            },
        ]);
        let multi_message = inspector_message(&multi);
        assert!(multi_message.contains("Metadata: 2 loaded, 0 unavailable"));
        assert!(multi_message.contains("3 immediate children"));
        assert!(multi_message.contains("9 known immediate bytes (non-recursive)"));
        assert!(!multi_message.contains("Owner UID"));
    }
}
