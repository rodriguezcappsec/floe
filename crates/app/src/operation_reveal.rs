use std::path::{Path, PathBuf};

use crate::operation_control::BatchId;
use floe_core::JobId;

pub(crate) const MAX_OPERATION_REVEAL_PATHS: usize = 4_096;
pub(crate) const OPERATION_REVEAL_DURATION_MS: u64 = 1_800;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationRevealGroup {
    Job(JobId),
    Batch(BatchId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OperationRevealRequest {
    group: OperationRevealGroup,
    directory: PathBuf,
    result: PathBuf,
}

impl OperationRevealRequest {
    pub(crate) fn new(job_id: JobId, batch_id: Option<BatchId>, result: PathBuf) -> Option<Self> {
        let directory = result.parent()?.to_path_buf();
        Some(Self {
            group: batch_id.map_or(
                OperationRevealGroup::Job(job_id),
                OperationRevealGroup::Batch,
            ),
            directory,
            result,
        })
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingOperationReveal {
    group: OperationRevealGroup,
    directory: PathBuf,
    generation: Option<u64>,
    paths: Vec<PathBuf>,
}

impl PendingOperationReveal {
    pub(crate) fn from_request(request: OperationRevealRequest) -> Self {
        Self {
            group: request.group,
            directory: request.directory,
            generation: None,
            paths: vec![request.result],
        }
    }

    pub(crate) fn merge(&mut self, request: OperationRevealRequest) -> bool {
        if self.group != request.group || self.directory != request.directory {
            return false;
        }
        if !self.paths.contains(&request.result) && self.paths.len() < MAX_OPERATION_REVEAL_PATHS {
            self.paths.push(request.result);
        }
        self.generation = None;
        true
    }

    pub(crate) fn bind_generation(&mut self, generation: u64) {
        self.generation = Some(generation);
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn is_bound(&self) -> bool {
        self.generation.is_some()
    }

    pub(crate) fn unbind(&mut self) {
        self.generation = None;
    }

    pub(crate) fn matches(&self, generation: u64, directory: &Path) -> bool {
        self.generation == Some(generation) && self.directory == directory
    }

    pub(crate) fn visible_paths<'a, I>(&self, entries: I) -> Vec<PathBuf>
    where
        I: IntoIterator<Item = &'a Path>,
    {
        let visible = entries.into_iter().collect::<Vec<_>>();
        self.paths
            .iter()
            .filter(|path| visible.contains(&path.as_path()))
            .cloned()
            .collect()
    }

    #[cfg(test)]
    fn path_count(&self) -> usize {
        self.paths.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation_control::BatchId;
    use crate::state::TrackedOperation;
    use floe_core::{
        ConflictPolicy, CopyRequest, CreateRequest, FileIdentity, MoveRequest, RenameRequest,
        ReplaceMode, ReplaceRequest, SymlinkPolicy,
    };
    use std::num::NonZeroU64;
    use std::os::unix::ffi::OsStringExt;

    fn job(raw: u64) -> JobId {
        JobId::new(NonZeroU64::new(raw).expect("non-zero job"))
    }

    #[test]
    fn phase_6v_reveal_policy_is_exact_bounded_and_generation_bound() {
        let job = job(17);
        let raw = std::ffi::OsString::from_vec(b"result-\xff.txt".to_vec());
        let path = PathBuf::from("/tmp/floe").join(raw);
        let request = OperationRevealRequest::new(job, None, path.clone()).expect("request");
        let mut pending = PendingOperationReveal::from_request(request);

        assert!(!pending.matches(41, Path::new("/tmp/floe")));
        pending.bind_generation(42);
        assert!(!pending.matches(41, Path::new("/tmp/floe")));
        assert!(!pending.matches(42, Path::new("/tmp/other")));
        assert!(pending.matches(42, Path::new("/tmp/floe")));
        assert_eq!(pending.visible_paths([path.as_path()]), vec![path]);
        assert!(
            pending
                .visible_paths([Path::new("/tmp/floe/other")])
                .is_empty()
        );
    }

    #[test]
    fn phase_6v_reveal_policy_merges_only_one_batch_and_deduplicates() {
        let first_job = job(21);
        let second_job = job(22);
        let batch = BatchId::new(9).expect("batch");
        let first = PathBuf::from("/tmp/floe/one");
        let second = PathBuf::from("/tmp/floe/two");
        let mut pending = PendingOperationReveal::from_request(
            OperationRevealRequest::new(first_job, Some(batch), first.clone()).expect("first"),
        );

        assert!(pending.merge(
            OperationRevealRequest::new(second_job, Some(batch), first.clone()).expect("duplicate")
        ));
        assert!(pending.merge(
            OperationRevealRequest::new(second_job, Some(batch), second.clone()).expect("second")
        ));
        assert_eq!(
            pending.visible_paths([first.as_path(), second.as_path()]),
            vec![first, second]
        );

        let other_batch = BatchId::new(10).expect("other batch");
        assert!(
            !pending.merge(
                OperationRevealRequest::new(
                    second_job,
                    Some(other_batch),
                    PathBuf::from("/tmp/floe/three")
                )
                .expect("other")
            )
        );
    }

    #[test]
    fn phase_6v_reveal_policy_caps_large_batch_result_sets() {
        let batch = BatchId::new(31).expect("batch");
        let first = PathBuf::from("/tmp/floe/result-0");
        let mut pending = PendingOperationReveal::from_request(
            OperationRevealRequest::new(job(1), Some(batch), first).expect("first"),
        );
        for index in 1..=MAX_OPERATION_REVEAL_PATHS + 8 {
            assert!(
                pending.merge(
                    OperationRevealRequest::new(
                        job(u64::try_from(index + 1).expect("job id")),
                        Some(batch),
                        PathBuf::from(format!("/tmp/floe/result-{index}")),
                    )
                    .expect("batch result")
                )
            );
        }
        assert_eq!(pending.path_count(), MAX_OPERATION_REVEAL_PATHS);
    }

    #[test]
    fn phase_6v_operation_results_preserve_exact_committed_destinations() {
        let source = PathBuf::from("/tmp/floe/source");
        let copy_destination = PathBuf::from("/tmp/floe/copy");
        let move_destination = PathBuf::from("/tmp/floe/moved");
        let rename_destination = PathBuf::from("/tmp/floe/renamed");
        let created = PathBuf::from("/tmp/floe/created");
        let duplicate = PathBuf::from("/tmp/floe/source (copy)");
        let replaced = PathBuf::from("/tmp/floe/replaced");
        let identity = FileIdentity::from_components(1, 2, 0o100644, 3, 4, 5);

        let operations = [
            (
                TrackedOperation::Copy(CopyRequest::new(
                    &source,
                    &copy_destination,
                    ConflictPolicy::FailIfExists,
                    SymlinkPolicy::Preserve,
                )),
                copy_destination,
            ),
            (
                TrackedOperation::Move(MoveRequest::new(
                    &source,
                    &move_destination,
                    ConflictPolicy::FailIfExists,
                )),
                move_destination,
            ),
            (
                TrackedOperation::Rename(RenameRequest::new(
                    &source,
                    "renamed",
                    ConflictPolicy::FailIfExists,
                )),
                rename_destination,
            ),
            (
                TrackedOperation::Create(
                    CreateRequest::empty_file(&created).expect("create request"),
                ),
                created,
            ),
            (
                TrackedOperation::Create(
                    CreateRequest::duplicate(&source, &duplicate).expect("duplicate request"),
                ),
                duplicate,
            ),
            (
                TrackedOperation::Replace(ReplaceRequest::new(
                    &source,
                    &replaced,
                    "/tmp/floe/.floe-replace-backups/backup",
                    ReplaceMode::Copy,
                    SymlinkPolicy::Preserve,
                    identity,
                    identity,
                )),
                replaced,
            ),
        ];

        for (operation, expected) in operations {
            assert_eq!(operation.completed_result_path(), Some(expected));
        }
    }
}
