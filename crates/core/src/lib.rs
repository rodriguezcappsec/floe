//! GTK-independent filesystem and navigation foundations for Floe.

mod copy;
mod directory;
mod error;
mod jobs;
mod model;
mod move_operation;
mod navigation;

pub use copy::{
    ConflictPolicy, CopyCancellation, CopyError, CopyOutcome, CopyProgress, CopyRequest,
    SymlinkPolicy, execute_copy,
};
pub use directory::{enumerate_directory, enumerate_directory_with_cancel};
pub use error::DirectoryError;
pub use jobs::{
    InvalidJobProgress, JobCommand, JobCommandKind, JobEvent, JobEventKind, JobFailure,
    JobFailureKind, JobId, JobProgress, JobRecord, JobState, JobTransitionError, OperationId,
};
pub use model::{DirectoryEntry, DirectoryListing, EntryKind, ThumbnailState};
pub use move_operation::{
    MoveCancellation, MoveError, MoveOutcome, MoveRequest, RenameRequest, execute_move,
    execute_rename,
};
pub use navigation::NavigationState;
