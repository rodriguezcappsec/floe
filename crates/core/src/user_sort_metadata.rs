use std::fmt;

use crate::{DirectoryEntry, SortColumn};

pub const USER_SORT_METADATA_ENTRY_CAPACITY: usize = 4_096;
pub const USER_SORT_METADATA_VALUE_CAPACITY: usize = 4 * 1_024;

const RATING_XATTR: &str = "user.baloo.rating";
const TAGS_XATTR: &str = "user.xdg.tags";
const COMMENT_XATTR: &str = "user.xdg.comment";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSortMetadataError {
    TooManyEntries { count: usize, maximum: usize },
    Cancelled,
}

impl fmt::Display for UserSortMetadataError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyEntries { count, maximum } => write!(
                formatter,
                "metadata sorting is limited to {maximum} items (this folder has {count})"
            ),
            Self::Cancelled => formatter.write_str("metadata sorting was cancelled"),
        }
    }
}

impl std::error::Error for UserSortMetadataError {}

/// Enrich entries for an explicitly selected user-metadata sort.
///
/// This function performs bounded no-follow reads and is intended for an
/// application worker. It never logs paths or attribute values and retains no
/// data beyond the returned directory entries.
pub fn enrich_user_sort_metadata(
    entries: &mut [DirectoryEntry],
    column: SortColumn,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(), UserSortMetadataError> {
    if !column.needs_user_metadata() {
        return Ok(());
    }
    if entries.len() > USER_SORT_METADATA_ENTRY_CAPACITY {
        return Err(UserSortMetadataError::TooManyEntries {
            count: entries.len(),
            maximum: USER_SORT_METADATA_ENTRY_CAPACITY,
        });
    }

    for entry in entries {
        if cancelled() {
            return Err(UserSortMetadataError::Cancelled);
        }
        match column {
            SortColumn::Rating if !entry.rating_sort_metadata_loaded() => {
                let rating = read_bounded(entry.path(), RATING_XATTR).and_then(parse_rating);
                entry.set_rating_sort_metadata(rating);
            }
            SortColumn::Tags if !entry.tags_sort_metadata_loaded() => {
                let tags = read_bounded(entry.path(), TAGS_XATTR).filter(|value| !value.is_empty());
                entry.set_tags_sort_metadata(tags);
            }
            SortColumn::Comment if !entry.comment_sort_metadata_loaded() => {
                let comment =
                    read_bounded(entry.path(), COMMENT_XATTR).filter(|value| !value.is_empty());
                entry.set_comment_sort_metadata(comment);
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(unix)]
fn read_bounded(path: &std::path::Path, name: &str) -> Option<Box<[u8]>> {
    read_bounded_with(|buffer| rustix::fs::lgetxattr(path, name, buffer).ok())
}

fn read_bounded_with(mut read: impl FnMut(&mut [u8]) -> Option<usize>) -> Option<Box<[u8]>> {
    let mut buffer = [0_u8; USER_SORT_METADATA_VALUE_CAPACITY + 1];
    let length = read(&mut buffer)?;
    (length <= USER_SORT_METADATA_VALUE_CAPACITY)
        .then(|| buffer[..length].to_vec().into_boxed_slice())
}

#[cfg(not(unix))]
fn read_bounded(_path: &std::path::Path, _name: &str) -> Option<Box<[u8]>> {
    None
}

fn parse_rating(value: Box<[u8]>) -> Option<u8> {
    let text = std::str::from_utf8(&value).ok()?.trim();
    let rating = text.parse::<u8>().ok()?;
    (rating <= 10).then_some(rating)
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, fs, path::PathBuf};

    use tempfile::tempdir;

    use crate::{EntryKind, ThumbnailState};

    use super::*;

    fn entry(path: PathBuf) -> DirectoryEntry {
        let name = path.file_name().expect("fixture name").to_os_string();
        DirectoryEntry::new(
            path,
            name,
            EntryKind::RegularFile,
            Some(1),
            None,
            None,
            false,
            false,
            ThumbnailState::NotRequested,
        )
    }

    #[cfg(unix)]
    fn set(path: &std::path::Path, name: &str, value: &[u8]) {
        rustix::fs::lsetxattr(path, name, value, rustix::fs::XattrFlags::empty())
            .expect("fixture xattr");
    }

    #[test]
    fn phase_20b1_user_metadata_is_explicit_bounded_and_cancelled() {
        let root = tempdir().expect("temporary directory");
        let path = root.path().join("rated");
        fs::write(&path, b"x").expect("fixture file");
        let mut entries = vec![entry(path)];

        enrich_user_sort_metadata(&mut entries, SortColumn::Name, || false)
            .expect("ordinary sort does no enrichment");
        assert!(!entries[0].rating_sort_metadata_loaded());

        assert_eq!(
            enrich_user_sort_metadata(&mut entries, SortColumn::Rating, || true),
            Err(UserSortMetadataError::Cancelled)
        );
        assert!(!entries[0].rating_sort_metadata_loaded());

        let mut too_many = vec![entries[0].clone(); USER_SORT_METADATA_ENTRY_CAPACITY + 1];
        assert!(matches!(
            enrich_user_sort_metadata(&mut too_many, SortColumn::Tags, || false),
            Err(UserSortMetadataError::TooManyEntries { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn phase_20b1_user_metadata_reads_kde_xattrs_without_following_symlinks() {
        use std::os::unix::fs::symlink;

        let root = tempdir().expect("temporary directory");
        let path = root.path().join("rated");
        fs::write(&path, b"x").expect("fixture file");
        set(&path, RATING_XATTR, b"7");
        set(&path, TAGS_XATTR, b"alpha,beta");
        set(&path, COMMENT_XATTR, b"reviewed");

        let link = root.path().join("link");
        symlink(&path, &link).expect("fixture symlink");
        let mut entries = vec![entry(path), entry(link)];
        enrich_user_sort_metadata(&mut entries, SortColumn::Rating, || false)
            .expect("rating enrichment");
        assert_eq!(entries[0].rating(), Some(7));
        assert_eq!(entries[0].tags(), None);
        assert_eq!(entries[0].comment(), None);
        enrich_user_sort_metadata(&mut entries, SortColumn::Tags, || false)
            .expect("tags enrichment");
        assert_eq!(entries[0].tags(), Some(b"alpha,beta".as_slice()));
        assert_eq!(entries[0].comment(), None);
        enrich_user_sort_metadata(&mut entries, SortColumn::Comment, || false)
            .expect("comment enrichment");
        assert_eq!(entries[0].comment(), Some(b"reviewed".as_slice()));
        assert_eq!(entries[1].rating(), None);
        assert_eq!(entries[1].tags(), None);
        assert_eq!(entries[1].comment(), None);
    }

    #[cfg(unix)]
    #[test]
    fn phase_20b1_user_metadata_rejects_malformed_and_oversized_values() {
        let root = tempdir().expect("temporary directory");
        let malformed = root.path().join(OsString::from("malformed"));
        fs::write(&malformed, b"x").expect("fixture file");
        set(&malformed, RATING_XATTR, b"11");
        assert!(
            read_bounded_with(|buffer| {
                buffer.fill(b'x');
                Some(buffer.len())
            })
            .is_none()
        );
        set(&malformed, COMMENT_XATTR, b"");
        let mut entries = vec![entry(malformed)];

        for column in [SortColumn::Rating, SortColumn::Tags, SortColumn::Comment] {
            enrich_user_sort_metadata(&mut entries, column, || false)
                .expect("invalid values remain unknown");
        }
        assert_eq!(entries[0].rating(), None);
        assert_eq!(entries[0].tags(), None);
        assert_eq!(entries[0].comment(), None);
    }
}
