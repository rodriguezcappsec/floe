//! Bounded, path-safe matching for the current-folder filter.

use std::ffi::OsStr;

use regex::{Regex, bytes};
use thiserror::Error;

/// Maximum number of Unicode scalar values accepted from the filter field.
pub const FOLDER_FILTER_QUERY_CAPACITY: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderFilterMode {
    Text,
    Glob,
    Regex,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum FolderFilterError {
    #[error("filter text is longer than {FOLDER_FILTER_QUERY_CAPACITY} characters")]
    QueryTooLong,
    #[error("invalid glob pattern: {0}")]
    InvalidGlob(String),
    #[error("invalid regular expression: {0}")]
    InvalidRegex(String),
}

/// A matcher compiled once per application-worker request.
#[derive(Debug)]
pub struct FolderFilterPattern {
    mode: FolderFilterMode,
    text: String,
    folded_text: String,
    unicode_regex: Option<Regex>,
    byte_regex: Option<bytes::Regex>,
}

impl FolderFilterPattern {
    pub fn compile(mode: FolderFilterMode, query: &str) -> Result<Self, FolderFilterError> {
        if query.chars().count() > FOLDER_FILTER_QUERY_CAPACITY {
            return Err(FolderFilterError::QueryTooLong);
        }

        let (unicode_regex, byte_regex) = match mode {
            FolderFilterMode::Text => (None, None),
            FolderFilterMode::Glob => {
                let source = glob_regex_source(query)?;
                let unicode = Regex::new(&source)
                    .map_err(|error| FolderFilterError::InvalidGlob(error.to_string()))?;
                (Some(unicode), byte_regex(&source))
            }
            FolderFilterMode::Regex => {
                let unicode = Regex::new(query)
                    .map_err(|error| FolderFilterError::InvalidRegex(error.to_string()))?;
                (Some(unicode), byte_regex(query))
            }
        };

        Ok(Self {
            mode,
            text: query.to_owned(),
            folded_text: query.to_lowercase(),
            unicode_regex,
            byte_regex,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Matches the exact filename without converting the result back into a path.
    pub fn matches(&self, name: &OsStr) -> bool {
        if self.text.is_empty() {
            return true;
        }

        if let Some(name) = name.to_str() {
            return match self.mode {
                FolderFilterMode::Text => name.to_lowercase().contains(&self.folded_text),
                FolderFilterMode::Glob | FolderFilterMode::Regex => self
                    .unicode_regex
                    .as_ref()
                    .is_some_and(|pattern| pattern.is_match(name)),
            };
        }

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;

            let name = name.as_bytes();
            match self.mode {
                FolderFilterMode::Text => ascii_insensitive_contains(name, self.text.as_bytes()),
                FolderFilterMode::Glob | FolderFilterMode::Regex => self
                    .byte_regex
                    .as_ref()
                    .is_some_and(|pattern| pattern.is_match(name)),
            }
        }

        #[cfg(not(unix))]
        {
            false
        }
    }
}

fn byte_regex(source: &str) -> Option<bytes::Regex> {
    bytes::RegexBuilder::new(source).unicode(false).build().ok()
}

fn ascii_insensitive_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

fn glob_regex_source(pattern: &str) -> Result<String, FolderFilterError> {
    let mut result = String::from("(?s)\\A");
    let mut characters = pattern.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            '[' => {
                result.push('[');
                if characters
                    .peek()
                    .is_some_and(|next| matches!(next, '!' | '^'))
                {
                    characters.next();
                    result.push('^');
                }
                let mut closed = false;
                let mut first = true;
                for member in characters.by_ref() {
                    if member == ']' && !first {
                        result.push(']');
                        closed = true;
                        break;
                    }
                    if matches!(member, '\\' | ']') {
                        result.push('\\');
                    }
                    result.push(member);
                    first = false;
                }
                if !closed {
                    return Err(FolderFilterError::InvalidGlob(
                        "missing closing ']'".to_owned(),
                    ));
                }
            }
            '\\' => {
                let Some(escaped) = characters.next() else {
                    return Err(FolderFilterError::InvalidGlob("trailing escape".to_owned()));
                };
                result.push_str(&regex::escape(&escaped.to_string()));
            }
            literal => result.push_str(&regex::escape(&literal.to_string())),
        }
    }
    result.push_str("\\z");
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_13a_filter_supports_empty_text_glob_and_regex() {
        let name = OsStr::new("Holiday Photo.JPG");
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Text, "photo")
                .expect("valid text filter")
                .matches(name)
        );
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Glob, "*.JPG")
                .expect("valid glob filter")
                .matches(name)
        );
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Regex, "^Holiday.*JPG$")
                .expect("valid regex filter")
                .matches(name)
        );
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Glob, "")
                .expect("valid empty filter")
                .matches(name)
        );
    }

    #[test]
    fn phase_13a_filter_rejects_invalid_and_over_capacity_patterns() {
        assert!(matches!(
            FolderFilterPattern::compile(FolderFilterMode::Glob, "[abc"),
            Err(FolderFilterError::InvalidGlob(_))
        ));
        assert!(matches!(
            FolderFilterPattern::compile(FolderFilterMode::Regex, "("),
            Err(FolderFilterError::InvalidRegex(_))
        ));
        let over_capacity = "x".repeat(FOLDER_FILTER_QUERY_CAPACITY + 1);
        assert_eq!(
            FolderFilterPattern::compile(FolderFilterMode::Text, &over_capacity)
                .expect_err("over-capacity filter must fail"),
            FolderFilterError::QueryTooLong
        );
    }

    #[cfg(unix)]
    #[test]
    fn phase_13a_filter_matches_non_utf8_names_without_lossy_reconstruction() {
        use std::os::unix::ffi::OsStrExt;

        let name = OsStr::from_bytes(b"Report-\xff.TXT");
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Text, "report")
                .expect("valid raw-name text filter")
                .matches(name)
        );
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Glob, "*.TXT")
                .expect("valid raw-name glob filter")
                .matches(name)
        );
        assert!(
            FolderFilterPattern::compile(FolderFilterMode::Regex, r"\.TXT$")
                .expect("valid raw-name regex filter")
                .matches(name)
        );
    }
}
