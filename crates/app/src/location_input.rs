use std::{
    fmt,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use floe_core::{DirectoryError, NavigationState};

pub struct PendingLocation {
    pub generation: u64,
    pub previous_navigation: NavigationState,
    pub submitted_text: String,
}

impl PendingLocation {
    pub fn matches(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn restore(self, navigation: &mut NavigationState) -> String {
        *navigation = self.previous_navigation;
        self.submitted_text
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocationInputError {
    Empty,
    Relative,
}

impl fmt::Display for LocationInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Enter an absolute folder path.",
            Self::Relative => "Location must be an absolute path beginning with /.",
        })
    }
}

pub fn validate_location_input(input: &str) -> Result<PathBuf, LocationInputError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(LocationInputError::Empty);
    }

    let path = PathBuf::from(input);
    if !path.is_absolute() {
        return Err(LocationInputError::Relative);
    }

    Ok(path)
}

pub fn resolve_location_input(input: &str, current: &Path) -> Result<PathBuf, LocationInputError> {
    if input.trim() == location_text(current) {
        return Ok(current.to_path_buf());
    }

    validate_location_input(input)
}

pub fn location_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn location_failure_message(error: &DirectoryError) -> String {
    let kind = match error {
        DirectoryError::Open { source, .. }
        | DirectoryError::ReadEntry { source, .. }
        | DirectoryError::Metadata { source, .. } => Some(source.kind()),
        DirectoryError::Cancelled => None,
    };

    match kind {
        Some(ErrorKind::NotFound) => {
            "That folder does not exist. Check the path and try again.".into()
        }
        Some(ErrorKind::NotADirectory) => {
            "That location is not a directory. Enter a folder path instead.".into()
        }
        Some(ErrorKind::PermissionDenied) => {
            "Floe cannot read that folder. Check its permissions or choose another location.".into()
        }
        _ => format!("That folder could not be opened: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, io, os::unix::ffi::OsStringExt, path::PathBuf};

    use floe_core::DirectoryError;

    use super::{
        LocationInputError, PendingLocation, location_failure_message, location_text,
        resolve_location_input, validate_location_input,
    };

    #[test]
    fn phase_6h_location_input_rejects_empty_and_relative_text() {
        assert_eq!(
            validate_location_input("   "),
            Err(LocationInputError::Empty)
        );
        assert_eq!(
            validate_location_input("Documents/work"),
            Err(LocationInputError::Relative)
        );
    }

    #[test]
    fn phase_6h_location_input_trims_and_accepts_absolute_paths() {
        assert_eq!(
            validate_location_input("  /tmp/floe folder  "),
            Ok(PathBuf::from("/tmp/floe folder"))
        );
    }

    #[test]
    fn phase_6h_location_display_does_not_replace_original_non_utf8_path() {
        let original = PathBuf::from(OsString::from_vec(b"/tmp/floe-\xff".to_vec()));
        let displayed = location_text(&original);

        assert!(displayed.contains('\u{fffd}'));
        assert_eq!(original.as_os_str().as_encoded_bytes(), b"/tmp/floe-\xff");
        assert_ne!(PathBuf::from(displayed), original);
        assert_eq!(
            resolve_location_input(&location_text(&original), &original),
            Ok(original)
        );
    }

    #[test]
    fn phase_6h_location_failure_distinguishes_files_from_directories() {
        let error = DirectoryError::Open {
            path: PathBuf::from("/tmp/file.txt"),
            source: io::Error::from(io::ErrorKind::NotADirectory),
        };

        assert_eq!(
            location_failure_message(&error),
            "That location is not a directory. Enter a folder path instead."
        );
    }

    #[test]
    fn phase_6h_failed_generation_restores_exact_navigation_snapshot() {
        let mut navigation = floe_core::NavigationState::new(PathBuf::from("/home/floe"));
        let previous_navigation = navigation.clone();
        assert!(navigation.navigate_to(PathBuf::from("/tmp/not-a-folder")));
        let pending = PendingLocation {
            generation: 17,
            previous_navigation,
            submitted_text: "/tmp/not-a-folder".into(),
        };

        assert!(pending.matches(17));
        let submitted = pending.restore(&mut navigation);
        assert_eq!(navigation.current(), PathBuf::from("/home/floe"));
        assert!(!navigation.can_go_back());
        assert!(!navigation.can_go_forward());
        assert_eq!(submitted, "/tmp/not-a-folder");
    }
}
