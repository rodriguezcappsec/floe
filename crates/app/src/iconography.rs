use std::sync::OnceLock;

use floe_core::{DirectoryEntry, EntryKind};
use gtk::{gdk, gio};

pub const LIST_ICON_EDGE: i32 = 28;
const ICON_RESOURCE_ROOT: &str = "/io/github/floe/FileManager/icons";

static RESOURCE_REGISTRATION: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryIcon {
    Folder,
    FolderLink,
    Generic,
    FileLink,
    Document,
    Spreadsheet,
    Presentation,
    Image,
    Audio,
    Video,
    Archive,
    Code,
    Pdf,
    Executable,
}

impl EntryIcon {
    pub const ALL: [Self; 14] = [
        Self::Folder,
        Self::FolderLink,
        Self::Generic,
        Self::FileLink,
        Self::Document,
        Self::Spreadsheet,
        Self::Presentation,
        Self::Image,
        Self::Audio,
        Self::Video,
        Self::Archive,
        Self::Code,
        Self::Pdf,
        Self::Executable,
    ];

    pub const fn icon_name(self) -> &'static str {
        match self {
            Self::Folder => "floe-folder",
            Self::FolderLink => "floe-folder-link",
            Self::Generic => "floe-file-generic",
            Self::FileLink => "floe-file-link",
            Self::Document => "floe-file-document",
            Self::Spreadsheet => "floe-file-spreadsheet",
            Self::Presentation => "floe-file-presentation",
            Self::Image => "floe-file-image",
            Self::Audio => "floe-file-audio",
            Self::Video => "floe-file-video",
            Self::Archive => "floe-file-archive",
            Self::Code => "floe-file-code",
            Self::Pdf => "floe-file-pdf",
            Self::Executable => "floe-file-executable",
        }
    }

    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Folder | Self::FolderLink => "floe-icon-folder",
            Self::Image | Self::Audio | Self::Video => "floe-icon-media",
            Self::Archive => "floe-icon-archive",
            Self::Code | Self::Executable => "floe-icon-code",
            Self::Pdf | Self::Document | Self::Spreadsheet | Self::Presentation => {
                "floe-icon-document"
            }
            Self::Generic | Self::FileLink => "floe-icon-generic",
        }
    }
}

pub fn register(display: &gdk::Display) {
    ensure_resources();
    gtk::IconTheme::for_display(display).add_resource_path(ICON_RESOURCE_ROOT);
}

pub fn icon_for_entry(entry: &DirectoryEntry) -> EntryIcon {
    match entry.kind() {
        EntryKind::Directory => EntryIcon::Folder,
        EntryKind::SymbolicLink {
            target_is_directory: true,
        } => EntryIcon::FolderLink,
        EntryKind::SymbolicLink {
            target_is_directory: false,
        } => EntryIcon::FileLink,
        EntryKind::Other => EntryIcon::Generic,
        EntryKind::RegularFile if entry.is_executable() => EntryIcon::Executable,
        EntryKind::RegularFile => icon_for_extension(entry.path().extension()),
    }
}

pub const fn grid_icon_edge(thumbnail_edge: u16) -> i32 {
    match thumbnail_edge {
        0..=64 => 48,
        65..=80 => 54,
        81..=96 => 60,
        97..=112 => 66,
        113..=128 => 72,
        129..=160 => 80,
        _ => 88,
    }
}

fn ensure_resources() {
    RESOURCE_REGISTRATION.get_or_init(|| {
        gio::resources_register_include!("floe.gresource")
            .expect("compiled Floe icon resources must register");
    });
}

fn icon_for_extension(extension: Option<&std::ffi::OsStr>) -> EntryIcon {
    let Some(extension) = extension.and_then(std::ffi::OsStr::to_str) else {
        return EntryIcon::Generic;
    };

    if matches_extension(extension, &["pdf"]) {
        EntryIcon::Pdf
    } else if matches_extension(
        extension,
        &[
            "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "ico", "svg", "svgz",
            "avif", "heic", "heif", "raw", "cr2", "nef",
        ],
    ) {
        EntryIcon::Image
    } else if matches_extension(
        extension,
        &["mp3", "ogg", "flac", "wav", "m4a", "aac", "opus"],
    ) {
        EntryIcon::Audio
    } else if matches_extension(
        extension,
        &["mp4", "mkv", "webm", "mov", "avi", "m4v", "flv"],
    ) {
        EntryIcon::Video
    } else if matches_extension(
        extension,
        &[
            "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", "tgz", "tbz", "txz", "deb", "rpm",
            "pkg", "iso",
        ],
    ) {
        EntryIcon::Archive
    } else if matches_extension(
        extension,
        &[
            "rs", "c", "h", "cpp", "hpp", "py", "js", "ts", "tsx", "jsx", "go", "java", "kt",
            "swift", "sh", "bash", "zsh", "fish", "toml", "yaml", "yml", "json", "xml", "html",
            "css", "scss", "sql",
        ],
    ) {
        EntryIcon::Code
    } else if matches_extension(extension, &["csv", "xls", "xlsx", "ods"]) {
        EntryIcon::Spreadsheet
    } else if matches_extension(extension, &["ppt", "pptx", "odp", "key"]) {
        EntryIcon::Presentation
    } else if matches_extension(
        extension,
        &["txt", "md", "rtf", "doc", "docx", "odt", "pages", "epub"],
    ) {
        EntryIcon::Document
    } else {
        EntryIcon::Generic
    }
}

fn matches_extension(extension: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

#[cfg(test)]
mod phase_6g_tests {
    use std::{fs, path::Path};

    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, os::unix::fs::PermissionsExt};

    use floe_core::{DirectoryEntry, enumerate_directory};
    use tempfile::tempdir;

    use super::*;

    fn entry_for<'a>(entries: &'a [DirectoryEntry], path: &Path) -> &'a DirectoryEntry {
        entries
            .iter()
            .find(|entry| entry.path() == path)
            .expect("fixture entry should be enumerated")
    }

    #[test]
    fn phase_6g_extension_policy_covers_reviewed_families_case_insensitively() {
        let cases = [
            ("report.PDF", EntryIcon::Pdf),
            ("photo.AvIf", EntryIcon::Image),
            ("album.FLAC", EntryIcon::Audio),
            ("clip.MkV", EntryIcon::Video),
            ("backup.TAR", EntryIcon::Archive),
            ("main.RS", EntryIcon::Code),
            ("budget.XLSX", EntryIcon::Spreadsheet),
            ("slides.ODP", EntryIcon::Presentation),
            ("notes.MD", EntryIcon::Document),
            ("unknown.floe", EntryIcon::Generic),
        ];

        for (name, expected) in cases {
            assert_eq!(icon_for_extension(Path::new(name).extension()), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn phase_6g_kind_and_executable_precedence_uses_enumerated_metadata() {
        let directory = tempdir().expect("temporary directory should be created");
        let folder = directory.path().join("folder.rs");
        let executable = directory.path().join("run.txt");
        let folder_link = directory.path().join("folder-link");
        let file_link = directory.path().join("file-link");
        fs::create_dir(&folder).expect("folder fixture should be created");
        fs::write(&executable, b"#!/bin/sh\n").expect("executable fixture should be written");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("executable mode should be set");
        std::os::unix::fs::symlink(&folder, &folder_link).expect("folder link should be created");
        std::os::unix::fs::symlink(&executable, &file_link).expect("file link should be created");

        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        assert_eq!(
            icon_for_entry(entry_for(listing.entries(), &folder)),
            EntryIcon::Folder
        );
        assert_eq!(
            icon_for_entry(entry_for(listing.entries(), &executable)),
            EntryIcon::Executable
        );
        assert_eq!(
            icon_for_entry(entry_for(listing.entries(), &folder_link)),
            EntryIcon::FolderLink
        );
        assert_eq!(
            icon_for_entry(entry_for(listing.entries(), &file_link)),
            EntryIcon::FileLink
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_6g_non_utf8_name_keeps_exact_path_and_classifies_ascii_extension() {
        let directory = tempdir().expect("temporary directory should be created");
        let raw_name = OsString::from_vec(vec![b'p', 0x80, b'.', b'P', b'D', b'F']);
        let path = directory.path().join(&raw_name);
        fs::write(&path, b"pdf marker").expect("non-UTF-8 fixture should be written");

        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        let entry = entry_for(listing.entries(), &path);
        assert_eq!(entry.path(), path);
        assert_eq!(icon_for_entry(entry), EntryIcon::Pdf);
    }

    #[test]
    fn phase_6g_list_and_grid_icon_edges_are_bounded_optical_sizes() {
        assert_eq!(LIST_ICON_EDGE, 28);
        assert_eq!(grid_icon_edge(64), 48);
        assert_eq!(grid_icon_edge(112), 66);
        assert_eq!(grid_icon_edge(192), 88);
        assert!(grid_icon_edge(192) < 192);
    }

    #[test]
    fn phase_6g_all_vector_resources_register_under_one_icon_family() {
        ensure_resources();
        for icon in EntryIcon::ALL {
            assert!(!icon.icon_name().ends_with("-symbolic"));
            let path = format!(
                "{ICON_RESOURCE_ROOT}/scalable/mimetypes/{}.svg",
                icon.icon_name()
            );
            let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
                .expect("compiled vector icon should be registered");
            assert!(!bytes.is_empty());
            assert!(bytes.as_ref().contains(&b'#'));
        }
    }
}
