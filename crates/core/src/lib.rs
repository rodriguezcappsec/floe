//! GTK-independent filesystem and navigation foundations for Floe.

mod copy;
mod directory;
mod error;
mod jobs;
mod model;
mod move_operation;
mod navigation;
mod permanent_delete;
mod sorting;
mod trash_lifecycle;

pub use copy::{
    ConflictPolicy, CopyCancellation, CopyError, CopyOutcome, CopyProgress, CopyRequest,
    SymlinkPolicy, execute_copy,
};
pub use directory::{enumerate_directory, enumerate_directory_with_cancel};
pub use error::DirectoryError;
pub use jobs::{
    InvalidJobProgress, JobCommand, JobCommandKind, JobEvent, JobEventKind, JobFailure,
    JobFailureKind, JobId, JobProgress, JobRecord, JobState, JobTransitionError, OperationId,
    ProgressUnit,
};
pub use model::{DirectoryEntry, DirectoryListing, EntryKind, ThumbnailState, TrashMetadata};
pub use move_operation::{
    FileIdentity, MoveCancellation, MoveError, MoveOutcome, MoveRequest, RenameRequest,
    execute_move, execute_move_with_progress, execute_rename,
};
pub use navigation::NavigationState;
pub use permanent_delete::{
    PermanentDeleteError, PermanentDeleteOutcome, PermanentDeleteProgress, PermanentDeleteRequest,
    PermanentDeleteRequestError, execute_permanent_delete,
};
pub use sorting::{DirectorySort, SortColumn, SortDirection};
pub use trash_lifecycle::{
    RestoreError, RestoreOutcome, RestoreRequest, RestoreRequestError, TrashEnumerateError,
    TrashRoot, enumerate_trash_with_cancel, execute_restore,
};
