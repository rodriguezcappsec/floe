use std::{ffi::OsStr, fs, path::Path};

#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};

use crate::{
    DirectoryEntry, DirectoryError, DirectoryListing, DirectorySort, EntryKind, ThumbnailState,
};

/// Enumerates one directory without following it recursively.
pub fn enumerate_directory(path: &Path) -> Result<DirectoryListing, DirectoryError> {
    enumerate_directory_with_cancel(path, || false)
}

/// Enumerates one directory and cooperatively stops when `is_cancelled` is true.
///
/// This lets the application supersede slow requests without coupling the core
/// to a particular async runtime or UI toolkit.
pub fn enumerate_directory_with_cancel(
    path: &Path,
    is_cancelled: impl Fn() -> bool,
) -> Result<DirectoryListing, DirectoryError> {
    let reader = fs::read_dir(path).map_err(|source| DirectoryError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let mut entries = Vec::new();

    for result in reader {
        if is_cancelled() {
            return Err(DirectoryError::Cancelled);
        }

        let entry = result.map_err(|source| DirectoryError::ReadEntry {
            path: path.to_path_buf(),
            source,
        })?;
        let entry_path = entry.path();
        let metadata =
            fs::symlink_metadata(&entry_path).map_err(|source| DirectoryError::Metadata {
                path: entry_path.clone(),
                source,
            })?;
        let file_type = metadata.file_type();
        let kind = if file_type.is_dir() {
            EntryKind::Directory
        } else if file_type.is_file() {
            EntryKind::RegularFile
        } else if file_type.is_symlink() {
            let target_is_directory = fs::metadata(&entry_path)
                .map(|target| target.is_dir())
                .unwrap_or(false);
            EntryKind::SymbolicLink {
                target_is_directory,
            }
        } else {
            EntryKind::Other
        };
        let name = entry.file_name();
        let hidden = is_hidden(&name);
        let size = matches!(kind, EntryKind::RegularFile).then_some(metadata.len());
        let modified = metadata.modified().ok();
        let created = metadata.created().ok();
        let accessed = metadata.accessed().ok();
        #[cfg(unix)]
        let executable =
            matches!(kind, EntryKind::RegularFile) && metadata.permissions().mode() & 0o111 != 0;
        #[cfg(not(unix))]
        let executable = false;

        entries.push(
            DirectoryEntry::new(
                entry_path,
                name,
                kind,
                size,
                modified,
                None,
                hidden,
                executable,
                ThumbnailState::NotRequested,
            )
            .with_additional_timestamps(created, accessed),
        );
    }

    DirectorySort::default().sort_entries(&mut entries);

    Ok(DirectoryListing::new(path.to_path_buf(), entries))
}

#[cfg(unix)]
fn is_hidden(name: &OsStr) -> bool {
    name.as_bytes().first() == Some(&b'.')
}

#[cfg(not(unix))]
fn is_hidden(name: &OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs};

    #[cfg(unix)]
    use std::os::unix::{ffi::OsStringExt, fs::symlink};

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn lists_directories_first_and_marks_hidden_entries() {
        let root = tempdir().expect("temporary directory should be created");
        fs::write(root.path().join("zeta.txt"), b"hello").expect("fixture file should be written");
        fs::create_dir(root.path().join("alpha")).expect("fixture directory should be created");
        fs::write(root.path().join(".secret"), b"hidden")
            .expect("hidden fixture should be written");

        let listing = enumerate_directory(root.path()).expect("fixture should be readable");
        let names: Vec<_> = listing
            .entries()
            .iter()
            .map(DirectoryEntry::display_name_lossy)
            .collect();

        assert_eq!(names, ["alpha", ".secret", "zeta.txt"]);
        assert!(listing.entries()[1].is_hidden());
        assert_eq!(listing.entries()[2].size(), Some(5));
    }

    #[cfg(unix)]
    #[test]
    fn preserves_non_utf8_names_and_recognizes_directory_symlinks() {
        let root = tempdir().expect("temporary directory should be created");
        let raw_name = OsString::from_vec(vec![b'f', b'o', 0x80]);
        fs::write(root.path().join(&raw_name), b"data")
            .expect("non UTF-8 fixture should be written");
        fs::create_dir(root.path().join("target")).expect("target directory should be created");
        symlink(root.path().join("target"), root.path().join("shortcut"))
            .expect("fixture symlink should be created");

        let listing = enumerate_directory(root.path()).expect("fixture should be readable");
        let non_utf8 = listing
            .entries()
            .iter()
            .find(|entry| entry.display_name() == raw_name)
            .expect("original non UTF-8 name should be retained");
        let shortcut = listing
            .entries()
            .iter()
            .find(|entry| entry.display_name() == "shortcut")
            .expect("symlink should be listed");

        assert_eq!(non_utf8.path(), root.path().join(&raw_name));
        assert!(shortcut.is_navigable_directory());
    }

    #[test]
    fn cancellation_stops_enumeration() {
        let root = tempdir().expect("temporary directory should be created");
        fs::write(root.path().join("one"), b"1").expect("fixture should be written");

        let result = enumerate_directory_with_cancel(root.path(), || true);
        assert!(matches!(result, Err(DirectoryError::Cancelled)));
    }
}
