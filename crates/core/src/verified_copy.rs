//! Optional copy-and-verify request and result policy.
//!
//! Execution lives in `floe-app`, where it can reuse the reviewed streaming
//! SHA-256 implementation without making the filesystem core depend on GLib.

use std::path::{Path, PathBuf};

use crate::{ConflictPolicy, CopyOutcome, CopyProgress, CopyRequest, SymlinkPolicy};

pub const VERIFIED_COPY_ENTRY_CAPACITY: usize = 4_096;
pub const VERIFIED_COPY_DEPTH_CAPACITY: usize = 64;
pub const VERIFIED_COPY_PATH_BYTES: usize = 1024 * 1024;

/// Exact request for an explicit copy-and-verify operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCopyRequest {
    copy: CopyRequest,
}

impl VerifiedCopyRequest {
    pub fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self {
            copy: CopyRequest::new(
                source,
                destination,
                ConflictPolicy::FailIfExists,
                SymlinkPolicy::Preserve,
            ),
        }
    }

    pub fn source(&self) -> &Path {
        self.copy.source()
    }

    pub fn destination(&self) -> &Path {
        self.copy.destination()
    }

    pub fn copy_request(&self) -> &CopyRequest {
        &self.copy
    }
}

/// Truthful publication state retained for every terminal outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedDestinationState {
    NotCreated,
    CopiedUnverified,
    Verified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedCopyStage {
    Planning,
    HashingSource,
    Copying,
    SyncingDestination,
    RevalidatingSource,
    HashingDestination,
    Verified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCopyProgress {
    stage: VerifiedCopyStage,
    completed: u64,
    total: Option<u64>,
}

impl VerifiedCopyProgress {
    pub const fn new(stage: VerifiedCopyStage, completed: u64, total: Option<u64>) -> Self {
        Self {
            stage,
            completed,
            total,
        }
    }

    pub const fn from_copy(progress: CopyProgress) -> Self {
        if progress.total_bytes() == 0 {
            Self::new(
                VerifiedCopyStage::Copying,
                progress.entries_copied(),
                Some(progress.total_entries()),
            )
        } else {
            Self::new(
                VerifiedCopyStage::Copying,
                progress.bytes_copied(),
                Some(progress.total_bytes()),
            )
        }
    }

    pub const fn stage(self) -> VerifiedCopyStage {
        self.stage
    }

    pub const fn completed(self) -> u64 {
        self.completed
    }

    pub const fn total(self) -> Option<u64> {
        self.total
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCopyOutcome {
    copy: CopyOutcome,
    regular_files_verified: u64,
    links_verified: u64,
}

impl VerifiedCopyOutcome {
    pub const fn new(copy: CopyOutcome, regular_files_verified: u64, links_verified: u64) -> Self {
        Self {
            copy,
            regular_files_verified,
            links_verified,
        }
    }

    pub const fn copy_outcome(self) -> CopyOutcome {
        self.copy
    }

    pub const fn regular_files_verified(self) -> u64 {
        self.regular_files_verified
    }

    pub const fn links_verified(self) -> u64 {
        self.links_verified
    }

    pub const fn destination_state(self) -> VerifiedDestinationState {
        VerifiedDestinationState::Verified
    }
}

/// Typed retry information; retries remain fail-if-exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCopyRetry {
    request: VerifiedCopyRequest,
    destination_state: VerifiedDestinationState,
}

impl VerifiedCopyRetry {
    pub fn new(request: VerifiedCopyRequest, destination_state: VerifiedDestinationState) -> Self {
        Self {
            request,
            destination_state,
        }
    }

    pub fn request(&self) -> &VerifiedCopyRequest {
        &self.request
    }

    pub const fn destination_state(&self) -> VerifiedDestinationState {
        self.destination_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_18v_copy_request_is_explicit_safe_and_retry_retains_partial_state() {
        let request = VerifiedCopyRequest::new(
            PathBuf::from("/tmp/source-raw"),
            PathBuf::from("/tmp/destination-raw"),
        );
        assert_eq!(request.source(), Path::new("/tmp/source-raw"));
        assert_eq!(request.destination(), Path::new("/tmp/destination-raw"));
        assert_eq!(
            request.copy_request().conflict_policy(),
            ConflictPolicy::FailIfExists
        );
        assert_eq!(
            request.copy_request().symlink_policy(),
            SymlinkPolicy::Preserve
        );

        let retry =
            VerifiedCopyRetry::new(request.clone(), VerifiedDestinationState::CopiedUnverified);
        assert_eq!(retry.request(), &request);
        assert_eq!(
            retry.destination_state(),
            VerifiedDestinationState::CopiedUnverified
        );
    }

    #[test]
    fn phase_18v_failure_progress_has_distinct_non_verified_stages() {
        for stage in [
            VerifiedCopyStage::Planning,
            VerifiedCopyStage::HashingSource,
            VerifiedCopyStage::Copying,
            VerifiedCopyStage::SyncingDestination,
            VerifiedCopyStage::RevalidatingSource,
            VerifiedCopyStage::HashingDestination,
        ] {
            assert_ne!(stage, VerifiedCopyStage::Verified);
        }
    }
}
