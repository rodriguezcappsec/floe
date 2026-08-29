use std::sync::OnceLock;

use floe_core::{DirectoryEntry, EntryKind};
use gtk::{gdk, gio};

pub const LIST_ICON_EDGE: i32 = 28;
pub const APPLICATION_ICON_NAME: &str = "io.github.rodriguezcappsec.Floe";
const ICON_RESOURCE_ROOT: &str = "/io/github/rodriguezcappsec/Floe/icons";
#[cfg(test)]
const APPLICATION_ICON_RESOURCE: &str =
    "/io/github/rodriguezcappsec/Floe/icons/512x512/apps/io.github.rodriguezcappsec.Floe.png";

static RESOURCE_REGISTRATION: OnceLock<()> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EntryIconStyle {
    #[default]
    FloeColor,
    Phosphor,
    System,
}

impl EntryIconStyle {
    pub const ALL: [Self; 3] = [Self::FloeColor, Self::Phosphor, Self::System];

    pub const fn persisted(self) -> &'static str {
        match self {
            Self::FloeColor => "floe-color",
            Self::Phosphor => "phosphor",
            Self::System => "system",
        }
    }

    pub fn from_persisted(value: &str) -> Option<Self> {
        match value.trim() {
            "floe-color" => Some(Self::FloeColor),
            "phosphor" => Some(Self::Phosphor),
            "system" => Some(Self::System),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::FloeColor => "Floe Color",
            Self::Phosphor => "Phosphor Monochrome",
            Self::System => "System Theme",
        }
    }

    pub const fn css_class(self) -> &'static str {
        match self {
            Self::FloeColor => "icon-style-floe-color",
            Self::Phosphor => "icon-style-phosphor",
            Self::System => "icon-style-system",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryIcon {
    Folder,
    FolderLink,
    Generic,
    FileLink,
    Text,
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
    pub const ALL: [Self; 15] = [
        Self::Folder,
        Self::FolderLink,
        Self::Generic,
        Self::FileLink,
        Self::Text,
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

    pub const fn icon_name(self, style: EntryIconStyle) -> &'static str {
        match style {
            EntryIconStyle::FloeColor => match self {
                Self::Folder => "floe-folder",
                Self::FolderLink => "floe-folder-link",
                Self::Generic => "floe-file-generic",
                Self::FileLink => "floe-file-link",
                Self::Text => "floe-file-text",
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
            },
            EntryIconStyle::Phosphor => match self {
                Self::Folder => "floe-phosphor-folder-symbolic",
                Self::FolderLink => "floe-phosphor-folder-open-symbolic",
                Self::Generic => "floe-phosphor-file-symbolic",
                Self::FileLink => "floe-phosphor-file-dashed-symbolic",
                Self::Text => "floe-phosphor-file-txt-symbolic",
                Self::Document => "floe-phosphor-file-doc-symbolic",
                Self::Spreadsheet => "floe-phosphor-file-xls-symbolic",
                Self::Presentation => "floe-phosphor-presentation-chart-symbolic",
                Self::Image => "floe-phosphor-file-image-symbolic",
                Self::Audio => "floe-phosphor-file-audio-symbolic",
                Self::Video => "floe-phosphor-file-video-symbolic",
                Self::Archive => "floe-phosphor-file-zip-symbolic",
                Self::Code => "floe-phosphor-file-code-symbolic",
                Self::Pdf => "floe-phosphor-file-pdf-symbolic",
                Self::Executable => "floe-phosphor-terminal-window-symbolic",
            },
            EntryIconStyle::System => self.system_icon_names()[0],
        }
    }

    pub const fn css_class(self) -> &'static str {
        match self {
            Self::Folder | Self::FolderLink => "floe-icon-folder",
            Self::Image | Self::Audio | Self::Video => "floe-icon-media",
            Self::Archive => "floe-icon-archive",
            Self::Code | Self::Executable => "floe-icon-code",
            Self::Pdf | Self::Text | Self::Document | Self::Spreadsheet | Self::Presentation => {
                "floe-icon-document"
            }
            Self::Generic | Self::FileLink => "floe-icon-generic",
        }
    }

    pub const fn system_icon_names(self) -> &'static [&'static str] {
        match self {
            Self::Folder => &["folder", "inode-directory", "floe-folder"],
            Self::FolderLink => &[
                "folder-symbolic-link",
                "folder-visiting",
                "floe-folder-link",
            ],
            Self::Generic => &["application-octet-stream", "unknown", "floe-file-generic"],
            Self::FileLink => &["emblem-symbolic-link", "floe-file-link"],
            Self::Text => &["text-plain", "text-x-generic", "floe-file-text"],
            Self::Document => &["x-office-document", "floe-file-document"],
            Self::Spreadsheet => &["x-office-spreadsheet", "floe-file-spreadsheet"],
            Self::Presentation => &["x-office-presentation", "floe-file-presentation"],
            Self::Image => &["image-x-generic", "floe-file-image"],
            Self::Audio => &["audio-x-generic", "floe-file-audio"],
            Self::Video => &["video-x-generic", "floe-file-video"],
            Self::Archive => &[
                "package-x-generic",
                "application-x-archive",
                "floe-file-archive",
            ],
            Self::Code => &["text-x-script", "text-x-source", "floe-file-code"],
            Self::Pdf => &["application-pdf", "floe-file-pdf"],
            Self::Executable => &[
                "application-x-executable",
                "application-x-shellscript",
                "floe-file-executable",
            ],
        }
    }
}

pub fn register(display: &gdk::Display) {
    ensure_resources();
    gtk::IconTheme::for_display(display).add_resource_path(ICON_RESOURCE_ROOT);
    gtk::Window::set_default_icon_name(APPLICATION_ICON_NAME);
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
        EntryKind::RegularFile => {
            let extension_icon = icon_for_extension(entry.path().extension());
            if extension_icon != EntryIcon::Generic {
                extension_icon
            } else if entry.is_executable() {
                EntryIcon::Executable
            } else {
                EntryIcon::Generic
            }
        }
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
    } else if matches_extension(extension, &["txt", "md", "markdown", "log", "nfo"]) {
        EntryIcon::Text
    } else if matches_extension(extension, &["rtf", "doc", "docx", "odt", "pages", "epub"]) {
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

    #[test]
    fn phase_13b_application_icon_uses_supplied_rgba_png_under_stable_name() {
        ensure_resources();
        assert_eq!(APPLICATION_ICON_NAME, "io.github.rodriguezcappsec.Floe");
        let bytes =
            gio::resources_lookup_data(APPLICATION_ICON_RESOURCE, gio::ResourceLookupFlags::NONE)
                .expect("compiled application icon should be registered");
        assert!(bytes.as_ref().starts_with(b"\x89PNG\r\n\x1a\n"));
    }

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
            ("notes.MD", EntryIcon::Text),
            ("letter.DOCX", EntryIcon::Document),
            ("unknown.floe", EntryIcon::Generic),
        ];

        for (name, expected) in cases {
            assert_eq!(icon_for_extension(Path::new(name).extension()), expected);
        }
    }

    #[cfg(unix)]
    #[test]
    fn phase_6g_kind_and_executable_fallback_uses_enumerated_metadata() {
        let directory = tempdir().expect("temporary directory should be created");
        let folder = directory.path().join("folder.rs");
        let executable = directory.path().join("run");
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
    fn post_phase_14_synthetic_execute_bits_do_not_hide_known_file_types() {
        let directory = tempdir().expect("temporary directory should be created");
        let cases = [
            ("manual.pdf", EntryIcon::Pdf),
            ("notes.txt", EntryIcon::Text),
            ("letter.docx", EntryIcon::Document),
            ("photo.png", EntryIcon::Image),
            ("track.flac", EntryIcon::Audio),
            ("clip.mkv", EntryIcon::Video),
            ("backup.zip", EntryIcon::Archive),
            ("main.rs", EntryIcon::Code),
            ("budget.xlsx", EntryIcon::Spreadsheet),
            ("slides.odp", EntryIcon::Presentation),
        ];

        for (name, _) in cases {
            let path = directory.path().join(name);
            fs::write(&path, b"fixture").expect("fixture should be written");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("synthetic executable mode should be set");
        }
        let unknown_executable = directory.path().join("tool.AppImage");
        fs::write(&unknown_executable, b"AppImage fixture")
            .expect("AppImage fixture should be written");
        fs::set_permissions(&unknown_executable, fs::Permissions::from_mode(0o755))
            .expect("AppImage executable mode should be set");

        let listing = enumerate_directory(directory.path()).expect("directory should enumerate");
        for (name, expected) in cases {
            let path = directory.path().join(name);
            assert_eq!(
                icon_for_entry(entry_for(listing.entries(), &path)),
                expected,
                "known extension {name} must outrank synthetic execute bits"
            );
        }
        assert_eq!(
            icon_for_entry(entry_for(listing.entries(), &unknown_executable)),
            EntryIcon::Executable
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
            let icon_name = icon.icon_name(EntryIconStyle::FloeColor);
            assert!(!icon_name.ends_with("-symbolic"));
            let path = format!("{ICON_RESOURCE_ROOT}/scalable/mimetypes/{}.svg", icon_name);
            let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
                .expect("compiled vector icon should be registered");
            assert!(!bytes.is_empty());
            assert!(bytes.as_ref().contains(&b'#'));
        }
    }

    #[test]
    fn post_phase_14_icon_styles_are_stable_and_phosphor_resources_are_symbolic() {
        assert_eq!(EntryIconStyle::default(), EntryIconStyle::FloeColor);
        assert_eq!(
            EntryIconStyle::ALL.map(EntryIconStyle::persisted),
            ["floe-color", "phosphor", "system"]
        );
        assert_eq!(
            EntryIconStyle::ALL.map(EntryIconStyle::label),
            ["Floe Color", "Phosphor Monochrome", "System Theme"]
        );
        assert_eq!(EntryIconStyle::from_persisted("unknown"), None);

        ensure_resources();
        let phosphor_resources = gio::resources_enumerate_children(
            &format!("{ICON_RESOURCE_ROOT}/scalable/actions"),
            gio::ResourceLookupFlags::NONE,
        )
        .expect("compiled Phosphor resource directory should enumerate");
        assert_eq!(phosphor_resources.len(), 45);
        for resource in &phosphor_resources {
            assert!(resource.starts_with("floe-phosphor-"));
            assert!(resource.ends_with("-symbolic.svg"));
            let path = format!("{ICON_RESOURCE_ROOT}/scalable/actions/{resource}");
            let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
                .expect("compiled Phosphor action icon should resolve");
            assert!(
                bytes
                    .as_ref()
                    .windows(b"currentColor".len())
                    .any(|part| part == b"currentColor")
            );
        }

        for icon in EntryIcon::ALL {
            let icon_name = icon.icon_name(EntryIconStyle::Phosphor);
            assert!(icon_name.ends_with("-symbolic"));
            let path = format!("{ICON_RESOURCE_ROOT}/scalable/actions/{icon_name}.svg");
            let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE)
                .expect("compiled Phosphor vector icon should be registered");
            assert!(
                bytes
                    .as_ref()
                    .windows(b"currentColor".len())
                    .any(|part| { part == b"currentColor" })
            );
        }

        for icon in EntryIcon::ALL {
            assert!(!icon.icon_name(EntryIconStyle::System).starts_with("floe-"));
            assert_eq!(
                icon.system_icon_names().first().copied(),
                Some(icon.icon_name(EntryIconStyle::System))
            );
            assert!(icon.system_icon_names().len() >= 2);
            assert_eq!(
                icon.system_icon_names().last().copied(),
                Some(icon.icon_name(EntryIconStyle::FloeColor)),
                "every system family must end in its distinct app-owned fallback"
            );
        }
    }

    #[test]
    fn post_phase_14_file_type_icon_pdf_text_and_documents_remain_distinct() {
        assert_eq!(
            icon_for_extension(Path::new("manual.pdf").extension()),
            EntryIcon::Pdf
        );
        assert_eq!(
            icon_for_extension(Path::new("notes.txt").extension()),
            EntryIcon::Text
        );
        assert_eq!(
            icon_for_extension(Path::new("letter.docx").extension()),
            EntryIcon::Document
        );

        for style in [EntryIconStyle::FloeColor, EntryIconStyle::Phosphor] {
            let names = [
                EntryIcon::Pdf.icon_name(style),
                EntryIcon::Text.icon_name(style),
                EntryIcon::Document.icon_name(style),
            ];
            assert_ne!(names[0], names[1]);
            assert_ne!(names[0], names[2]);
            assert_ne!(names[1], names[2]);
        }

        let pdf_fallback = EntryIcon::Pdf.system_icon_names().last();
        let text_fallback = EntryIcon::Text.system_icon_names().last();
        let document_fallback = EntryIcon::Document.system_icon_names().last();
        assert_ne!(pdf_fallback, text_fallback);
        assert_ne!(pdf_fallback, document_fallback);
        assert_ne!(text_fallback, document_fallback);
    }
}
