use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    os::unix::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

pub const PERMISSION_TARGET_CAPACITY: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionScope {
    Direct,
    Recursive,
}

pub const PERMISSION_IDENTITY_NAME_CAPACITY: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionIdentity {
    Id(u32),
    LocalName(OsString),
}

impl PermissionIdentity {
    pub fn local_name(name: OsString) -> Result<Self, PermissionRequestError> {
        validate_identity_name(&name)?;
        Ok(Self::LocalName(name))
    }
}

impl From<u32> for PermissionIdentity {
    fn from(value: u32) -> Self {
        Self::Id(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionChange {
    pub file_mode: Option<u32>,
    pub directory_mode: Option<u32>,
    pub executable: Option<bool>,
    pub owner: Option<PermissionIdentity>,
    pub group: Option<PermissionIdentity>,
}

impl PermissionChange {
    pub fn new(
        file_mode: Option<u32>,
        directory_mode: Option<u32>,
        executable: Option<bool>,
        owner: Option<PermissionIdentity>,
        group: Option<PermissionIdentity>,
    ) -> Result<Self, PermissionRequestError> {
        if file_mode.is_none()
            && directory_mode.is_none()
            && executable.is_none()
            && owner.is_none()
            && group.is_none()
        {
            return Err(PermissionRequestError::NoChange);
        }
        if executable.is_some() && (file_mode.is_some() || directory_mode.is_some()) {
            return Err(PermissionRequestError::AmbiguousMode);
        }
        for mode in [file_mode, directory_mode].into_iter().flatten() {
            if mode > 0o7777 {
                return Err(PermissionRequestError::InvalidMode(mode));
            }
        }
        Ok(Self {
            file_mode,
            directory_mode,
            executable,
            owner,
            group,
        })
    }

    pub const fn changes_ownership(&self) -> bool {
        self.owner.is_some() || self.group.is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionRequest {
    targets: Vec<PathBuf>,
    scope: PermissionScope,
    change: PermissionChange,
}

impl PermissionRequest {
    pub fn new(
        targets: Vec<PathBuf>,
        scope: PermissionScope,
        change: PermissionChange,
    ) -> Result<Self, PermissionRequestError> {
        if targets.is_empty() || targets.len() > PERMISSION_TARGET_CAPACITY {
            return Err(PermissionRequestError::InvalidTargetCount);
        }
        let mut seen = HashSet::with_capacity(targets.len());
        for target in &targets {
            validate_target(target)?;
            if !seen.insert(target.clone()) {
                return Err(PermissionRequestError::Duplicate(target.clone()));
            }
        }
        for (index, target) in targets.iter().enumerate() {
            for other in targets.iter().skip(index + 1) {
                if target.starts_with(other) || other.starts_with(target) {
                    return Err(PermissionRequestError::Nested {
                        first: target.clone(),
                        second: other.clone(),
                    });
                }
            }
        }
        Ok(Self {
            targets,
            scope,
            change,
        })
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }
    pub const fn scope(&self) -> PermissionScope {
        self.scope
    }
    pub const fn change(&self) -> &PermissionChange {
        &self.change
    }
    pub const fn requires_confirmation(&self) -> bool {
        matches!(self.scope, PermissionScope::Recursive) || self.change.changes_ownership()
    }
}

fn validate_identity_name(name: &OsStr) -> Result<(), PermissionRequestError> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || bytes.len() > PERMISSION_IDENTITY_NAME_CAPACITY
        || bytes
            .iter()
            .any(|byte| matches!(byte, b':' | b'\n' | b'\r' | 0))
    {
        return Err(PermissionRequestError::InvalidIdentityName(
            name.to_os_string(),
        ));
    }
    Ok(())
}

fn validate_target(target: &Path) -> Result<(), PermissionRequestError> {
    if !target.is_absolute() {
        return Err(PermissionRequestError::Relative(target.to_path_buf()));
    }
    if target.file_name().is_none() {
        return Err(PermissionRequestError::ProtectedRoot(target.to_path_buf()));
    }
    if target.components().any(|component| {
        matches!(
            component,
            Component::CurDir | Component::ParentDir | Component::Prefix(_)
        )
    }) {
        return Err(PermissionRequestError::Unnormalized(target.to_path_buf()));
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PermissionRequestError {
    #[error("select between one and {PERMISSION_TARGET_CAPACITY} permission targets")]
    InvalidTargetCount,
    #[error("permission target must be absolute: {}", .0.display())]
    Relative(PathBuf),
    #[error("filesystem roots cannot be permission targets: {}", .0.display())]
    ProtectedRoot(PathBuf),
    #[error("permission target is not lexically normalized: {}", .0.display())]
    Unnormalized(PathBuf),
    #[error("duplicate permission target: {}", .0.display())]
    Duplicate(PathBuf),
    #[error("nested permission targets are ambiguous: {} and {}", first.display(), second.display())]
    Nested { first: PathBuf, second: PathBuf },
    #[error("choose at least one permission or ownership change")]
    NoChange,
    #[error("explicit modes and executable toggle cannot be combined")]
    AmbiguousMode,
    #[error("mode must be between 0000 and 7777, got {0:o}")]
    InvalidMode(u32),
    #[error("local owner/group name is invalid")]
    InvalidIdentityName(OsString),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_10d_permission_request_validates_paths_modes_identity_and_scope() {
        let change = PermissionChange::new(
            Some(0o640),
            Some(0o750),
            None,
            Some(1000.into()),
            Some(PermissionIdentity::local_name(OsString::from("users")).expect("local group")),
        )
        .expect("change");
        let request = PermissionRequest::new(
            vec![PathBuf::from("/tmp/file")],
            PermissionScope::Recursive,
            change.clone(),
        )
        .expect("request");
        assert!(request.requires_confirmation());
        assert_eq!(request.change().file_mode, Some(0o640));
        assert!(matches!(
            request.change().group,
            Some(PermissionIdentity::LocalName(ref name)) if name == "users"
        ));
        assert!(matches!(
            PermissionIdentity::local_name(OsString::from("bad:name")),
            Err(PermissionRequestError::InvalidIdentityName(_))
        ));
        assert!(matches!(
            PermissionChange::new(Some(0o10000), None, None, None, None),
            Err(PermissionRequestError::InvalidMode(_))
        ));
        assert_eq!(
            PermissionChange::new(None, None, None, None, None),
            Err(PermissionRequestError::NoChange)
        );
        assert_eq!(
            PermissionChange::new(Some(0o600), None, Some(true), None, None),
            Err(PermissionRequestError::AmbiguousMode)
        );
        assert!(matches!(
            PermissionRequest::new(
                vec![PathBuf::from("/")],
                PermissionScope::Direct,
                change.clone()
            ),
            Err(PermissionRequestError::ProtectedRoot(_))
        ));
        assert!(matches!(
            PermissionRequest::new(
                vec![PathBuf::from("/tmp/a"), PathBuf::from("/tmp/a/b")],
                PermissionScope::Recursive,
                change
            ),
            Err(PermissionRequestError::Nested { .. })
        ));
    }
}
