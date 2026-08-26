//! GTK-independent advanced search predicates.

use std::{ffi::OsStr, time::SystemTime};

use thiserror::Error;

use crate::{DirectoryEntry, EntryKind};

pub const ADVANCED_FILTER_EXTENSION_CAPACITY: usize = 64;
pub const ADVANCED_FILTER_MIME_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EntryTypeFilter {
    #[default]
    Any,
    File,
    Folder,
    SymbolicLink,
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HiddenFilter {
    /// Use the browser's ordinary Show Hidden setting.
    #[default]
    CurrentSetting,
    /// Include hidden and non-hidden entries.
    Include,
    /// Match hidden entries only.
    Only,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerFilter {
    Uid(u32),
}

/// Structured rather than stringly typed so later facets, including tags, can be
/// added without changing filename/path identity or introducing a query parser.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdvancedFilter {
    pub entry_type: EntryTypeFilter,
    pub extension: Option<String>,
    pub mime: Option<String>,
    pub minimum_size: Option<u64>,
    pub maximum_size: Option<u64>,
    pub modified_after: Option<SystemTime>,
    pub modified_before: Option<SystemTime>,
    pub owner: Option<OwnerFilter>,
    pub hidden: HiddenFilter,
    pub match_case: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AdvancedMetadataNeeds {
    pub mime: bool,
    pub owner: bool,
}

impl AdvancedMetadataNeeds {
    pub const fn any(self) -> bool {
        self.mime || self.owner
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AdvancedMetadata {
    pub mime: Option<String>,
    pub owner_uid: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvancedFilterDecision {
    Match,
    NoMatch,
    NeedsMetadata(AdvancedMetadataNeeds),
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AdvancedFilterError {
    #[error("extension is longer than {ADVANCED_FILTER_EXTENSION_CAPACITY} characters")]
    ExtensionTooLong,
    #[error("MIME filter is longer than {ADVANCED_FILTER_MIME_CAPACITY} characters")]
    MimeTooLong,
    #[error("MIME filter must be a type/subtype or type/* pattern")]
    InvalidMime,
    #[error("minimum size cannot exceed maximum size")]
    InvalidSizeRange,
    #[error("modified-after date cannot be later than modified-before date")]
    InvalidDateRange,
}

impl AdvancedFilter {
    pub fn validate(&self) -> Result<(), AdvancedFilterError> {
        if self
            .extension
            .as_deref()
            .is_some_and(|value| value.chars().count() > ADVANCED_FILTER_EXTENSION_CAPACITY)
        {
            return Err(AdvancedFilterError::ExtensionTooLong);
        }
        if let Some(mime) = self.mime.as_deref() {
            if mime.chars().count() > ADVANCED_FILTER_MIME_CAPACITY {
                return Err(AdvancedFilterError::MimeTooLong);
            }
            if !valid_mime_pattern(mime) {
                return Err(AdvancedFilterError::InvalidMime);
            }
        }
        if self
            .minimum_size
            .zip(self.maximum_size)
            .is_some_and(|(minimum, maximum)| minimum > maximum)
        {
            return Err(AdvancedFilterError::InvalidSizeRange);
        }
        if self
            .modified_after
            .zip(self.modified_before)
            .is_some_and(|(after, before)| after > before)
        {
            return Err(AdvancedFilterError::InvalidDateRange);
        }
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.entry_type != EntryTypeFilter::Any
            || self
                .extension
                .as_deref()
                .is_some_and(|value| !value.is_empty())
            || self.mime.as_deref().is_some_and(|value| !value.is_empty())
            || self.minimum_size.is_some()
            || self.maximum_size.is_some()
            || self.modified_after.is_some()
            || self.modified_before.is_some()
            || self.owner.is_some()
            || self.hidden != HiddenFilter::CurrentSetting
    }

    pub const fn metadata_needs(&self) -> AdvancedMetadataNeeds {
        AdvancedMetadataNeeds {
            mime: self.mime.is_some(),
            owner: self.owner.is_some(),
        }
    }

    /// Runs cheap entry predicates first and requests only missing lazy facts.
    /// When a required fact was attempted but remains unknown, the entry does not
    /// match; unknown metadata never broadens results silently.
    pub fn evaluate(
        &self,
        entry: &DirectoryEntry,
        metadata: Option<&AdvancedMetadata>,
    ) -> AdvancedFilterDecision {
        if !self.matches_kind(entry.kind())
            || !self.matches_hidden(entry.is_hidden())
            || !self.matches_extension(entry.display_name())
            || !self.matches_size(entry.size())
            || !self.matches_modified(entry.modified())
        {
            return AdvancedFilterDecision::NoMatch;
        }

        let mut needs = AdvancedMetadataNeeds::default();
        if let Some(pattern) = self.mime.as_deref() {
            match metadata.and_then(|facts| facts.mime.as_deref()) {
                Some(mime) if text_equal_or_mime_wildcard(mime, pattern, self.match_case) => {}
                Some(_) => return AdvancedFilterDecision::NoMatch,
                None if metadata.is_some() => return AdvancedFilterDecision::NoMatch,
                None => needs.mime = true,
            }
        }
        if let Some(OwnerFilter::Uid(expected)) = self.owner {
            match metadata.and_then(|facts| facts.owner_uid) {
                Some(actual) if actual == expected => {}
                Some(_) => return AdvancedFilterDecision::NoMatch,
                None if metadata.is_some() => return AdvancedFilterDecision::NoMatch,
                None => needs.owner = true,
            }
        }

        if needs.any() {
            AdvancedFilterDecision::NeedsMetadata(needs)
        } else {
            AdvancedFilterDecision::Match
        }
    }

    fn matches_kind(&self, kind: EntryKind) -> bool {
        match self.entry_type {
            EntryTypeFilter::Any => true,
            EntryTypeFilter::File => matches!(kind, EntryKind::RegularFile),
            EntryTypeFilter::Folder => matches!(kind, EntryKind::Directory),
            EntryTypeFilter::SymbolicLink => matches!(kind, EntryKind::SymbolicLink { .. }),
            EntryTypeFilter::Other => matches!(kind, EntryKind::Other),
        }
    }

    fn matches_hidden(&self, hidden: bool) -> bool {
        match self.hidden {
            HiddenFilter::CurrentSetting | HiddenFilter::Include => true,
            HiddenFilter::Only => hidden,
        }
    }

    fn matches_extension(&self, name: &OsStr) -> bool {
        let Some(expected) = self
            .extension
            .as_deref()
            .map(|value| value.trim_start_matches('.'))
            .filter(|value| !value.is_empty())
        else {
            return true;
        };
        let Some(actual) = std::path::Path::new(name).extension() else {
            return false;
        };
        os_str_equals(actual, expected, self.match_case)
    }

    fn matches_size(&self, size: Option<u64>) -> bool {
        if self.minimum_size.is_none() && self.maximum_size.is_none() {
            return true;
        }
        let Some(size) = size else {
            return false;
        };
        self.minimum_size.is_none_or(|minimum| size >= minimum)
            && self.maximum_size.is_none_or(|maximum| size <= maximum)
    }

    fn matches_modified(&self, modified: Option<SystemTime>) -> bool {
        if self.modified_after.is_none() && self.modified_before.is_none() {
            return true;
        }
        let Some(modified) = modified else {
            return false;
        };
        self.modified_after.is_none_or(|after| modified >= after)
            && self.modified_before.is_none_or(|before| modified <= before)
    }
}

fn valid_mime_pattern(pattern: &str) -> bool {
    let Some((top, subtype)) = pattern.split_once('/') else {
        return false;
    };
    !top.is_empty()
        && !subtype.is_empty()
        && !top.contains(char::is_whitespace)
        && !subtype.contains(char::is_whitespace)
        && !top.contains('*')
        && !subtype.contains('/')
        && (subtype == "*" || !subtype.contains('*'))
}

fn text_equal_or_mime_wildcard(actual: &str, expected: &str, match_case: bool) -> bool {
    let matches = |left: &str, right: &str| {
        if match_case {
            left == right
        } else {
            left.eq_ignore_ascii_case(right)
        }
    };
    let Some((expected_top, expected_subtype)) = expected.split_once('/') else {
        return false;
    };
    let Some((actual_top, actual_subtype)) = actual.split_once('/') else {
        return false;
    };
    matches(actual_top, expected_top)
        && (expected_subtype == "*" || matches(actual_subtype, expected_subtype))
}

fn os_str_equals(actual: &OsStr, expected: &str, match_case: bool) -> bool {
    match actual.to_str() {
        Some(actual) if match_case => actual == expected,
        Some(actual) => actual.to_lowercase() == expected.to_lowercase(),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                let actual = actual.as_bytes();
                let expected = expected.as_bytes();
                if match_case {
                    actual == expected
                } else {
                    actual.len() == expected.len()
                        && actual
                            .iter()
                            .zip(expected)
                            .all(|(left, right)| left.eq_ignore_ascii_case(right))
                }
            }
            #[cfg(not(unix))]
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, time::Duration};

    use crate::ThumbnailState;

    use super::*;

    fn entry(name: OsString, kind: EntryKind, size: Option<u64>, hidden: bool) -> DirectoryEntry {
        DirectoryEntry::new(
            std::path::PathBuf::from("/tmp").join(&name),
            name,
            kind,
            size,
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(200)),
            None,
            hidden,
            false,
            ThumbnailState::NotRequested,
        )
    }

    #[test]
    fn phase_13c_combines_type_extension_size_date_and_hidden() {
        let filter = AdvancedFilter {
            entry_type: EntryTypeFilter::File,
            extension: Some("TXT".to_owned()),
            minimum_size: Some(10),
            maximum_size: Some(20),
            modified_after: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(100)),
            hidden: HiddenFilter::Only,
            ..AdvancedFilter::default()
        };
        assert_eq!(
            filter.evaluate(
                &entry(
                    OsString::from(".notes.txt"),
                    EntryKind::RegularFile,
                    Some(15),
                    true,
                ),
                None,
            ),
            AdvancedFilterDecision::Match
        );
        assert_eq!(
            filter.evaluate(
                &entry(
                    OsString::from("notes.txt"),
                    EntryKind::RegularFile,
                    Some(15),
                    false,
                ),
                None,
            ),
            AdvancedFilterDecision::NoMatch
        );
    }

    #[test]
    fn phase_13c_requests_lazy_facts_and_unknown_never_matches() {
        let filter = AdvancedFilter {
            mime: Some("image/*".to_owned()),
            owner: Some(OwnerFilter::Uid(1000)),
            ..AdvancedFilter::default()
        };
        let candidate = entry(
            OsString::from("photo.png"),
            EntryKind::RegularFile,
            Some(4),
            false,
        );
        assert_eq!(
            filter.evaluate(&candidate, None),
            AdvancedFilterDecision::NeedsMetadata(AdvancedMetadataNeeds {
                mime: true,
                owner: true,
            })
        );
        assert_eq!(
            filter.evaluate(&candidate, Some(&AdvancedMetadata::default())),
            AdvancedFilterDecision::NoMatch
        );
        assert_eq!(
            filter.evaluate(
                &candidate,
                Some(&AdvancedMetadata {
                    mime: Some("image/png".to_owned()),
                    owner_uid: Some(1000),
                }),
            ),
            AdvancedFilterDecision::Match
        );
    }

    #[test]
    fn phase_13c_validates_ranges_mime_and_non_utf8_extension() {
        assert_eq!(
            AdvancedFilter {
                minimum_size: Some(2),
                maximum_size: Some(1),
                ..AdvancedFilter::default()
            }
            .validate(),
            Err(AdvancedFilterError::InvalidSizeRange)
        );
        assert_eq!(
            AdvancedFilter {
                mime: Some("image".to_owned()),
                ..AdvancedFilter::default()
            }
            .validate(),
            Err(AdvancedFilterError::InvalidMime)
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let filter = AdvancedFilter {
                extension: Some("TXT".to_owned()),
                ..AdvancedFilter::default()
            };
            assert_eq!(
                filter.evaluate(
                    &entry(
                        OsString::from_vec(b"report-\xff.TXT".to_vec()),
                        EntryKind::RegularFile,
                        Some(1),
                        false,
                    ),
                    None,
                ),
                AdvancedFilterDecision::Match
            );
        }
    }
}
