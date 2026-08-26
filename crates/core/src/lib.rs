//! GTK-independent filesystem and navigation foundations for Floe.

mod archive;
mod batch_rename;
mod checksum;
mod copy;
mod create_operation;
mod directory;
mod error;
mod jobs;
mod miller;
mod model;
mod move_operation;
mod navigation;
mod permanent_delete;
mod permissions;
mod session;
mod sorting;
mod split;
mod tabs;
mod trash_lifecycle;
mod view;

pub use archive::{
    ARCHIVE_LIST_RESULT_CAPACITY, ARCHIVE_MEMBER_DEPTH, ARCHIVE_MEMBER_PATH_BYTES,
    ARCHIVE_SOURCE_CAPACITY, ArchiveCancellation, ArchiveError, ArchiveFormat, ArchiveLimits,
    ArchiveMember, ArchiveMemberKind, ArchiveOutcome, ArchiveProgress, ArchiveRequest,
    ArchiveRequestError, execute_archive,
};
pub use batch_rename::{
    BATCH_RENAME_CAPACITY, BatchRenameCancellation, BatchRenameError, BatchRenameOutcome,
    BatchRenamePair, BatchRenameRequest, BatchRenameRequestError, execute_batch_rename,
};
pub use checksum::{
    CHECKSUM_TARGET_CAPACITY, ChecksumAlgorithm, ChecksumRequest, ChecksumRequestError,
    ExpectedDigest, encode_hex,
};
pub use copy::{
    ConflictPolicy, CopyCancellation, CopyError, CopyOutcome, CopyProgress, CopyRequest,
    SymlinkPolicy, execute_copy,
};
pub use create_operation::{
    CreateCancellation, CreateError, CreateKind, CreateOutcome, CreateProgress, CreateRequest,
    CreateRequestError, SymbolicLinkMode, execute_create,
};
pub use directory::{enumerate_directory, enumerate_directory_with_cancel};
pub use error::DirectoryError;
pub use jobs::{
    InvalidJobProgress, JobCommand, JobCommandKind, JobEvent, JobEventKind, JobFailure,
    JobFailureKind, JobId, JobProgress, JobRecord, JobState, JobTransitionError, OperationId,
    ProgressUnit,
};
pub use miller::{
    MILLER_COLUMN_CAPACITY, MillerChildKind, MillerColumn, MillerColumnDepth, MillerColumnModel,
    MillerReconcileTransition, MillerSelectionTransition, MillerStateError,
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
pub use permissions::{
    PERMISSION_IDENTITY_NAME_CAPACITY, PERMISSION_TARGET_CAPACITY, PermissionChange,
    PermissionIdentity, PermissionRequest, PermissionRequestError, PermissionScope,
};
pub use session::{
    BrowserSession, BrowserSessionId, SESSION_HISTORY_CAPACITY, SESSION_MAX_PATH_BYTES,
    SESSION_MAX_SERIALIZED_BYTES, SESSION_SELECTION_CAPACITY, SessionCodecError, SessionLocation,
    SessionScrollAnchor, SessionStateError,
};
pub use sorting::{
    DirectoryGrouping, DirectoryPlacement, DirectorySort, SortColumn, SortDirection,
};
pub use split::{
    BrowserSplit, SPLIT_RATIO_DEFAULT, SPLIT_RATIO_MAX, SPLIT_RATIO_MIN, SplitRatio, SplitSide,
    SplitStateError,
};
pub use tabs::{
    BrowserTabs, ClosedTab, RECENTLY_CLOSED_CAPACITY, TAB_CAPACITY, TabActivation, TabError,
    WORKSPACE_MAX_SERIALIZED_BYTES, WorkspaceCodecError,
};
pub use trash_lifecycle::{
    RestoreError, RestoreOutcome, RestoreRequest, RestoreRequestError, TrashEnumerateError,
    TrashRoot, enumerate_trash_with_cancel, execute_restore,
};
pub use view::{
    FileViewDensity, FolderViewState, GRID_SIZES, GridSize, ListColumn, ListColumnLayout, ViewMode,
};
