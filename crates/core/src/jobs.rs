use std::num::NonZeroU64;

use thiserror::Error;

/// Identifies one logical user-requested operation across retry attempts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(NonZeroU64);

impl OperationId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Identifies one execution attempt. A retry receives a new `JobId`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct JobId(NonZeroU64);

impl JobId {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobProgress {
    completed: u64,
    total: Option<NonZeroU64>,
}

impl JobProgress {
    pub const fn indeterminate() -> Self {
        Self {
            completed: 0,
            total: None,
        }
    }

    pub fn new(completed: u64, total: Option<u64>) -> Result<Self, InvalidJobProgress> {
        let total = match total {
            Some(0) => return Err(InvalidJobProgress::ZeroTotal),
            Some(value) if completed > value => {
                return Err(InvalidJobProgress::CompletedExceedsTotal {
                    completed,
                    total: value,
                });
            }
            Some(value) => NonZeroU64::new(value),
            None => None,
        };
        Ok(Self { completed, total })
    }

    pub const fn completed(self) -> u64 {
        self.completed
    }

    pub const fn total(self) -> Option<u64> {
        match self.total {
            Some(total) => Some(total.get()),
            None => None,
        }
    }

    pub fn fraction(self) -> Option<f64> {
        self.total
            .map(|total| self.completed as f64 / total.get() as f64)
    }
}

impl Default for JobProgress {
    fn default() -> Self {
        Self::indeterminate()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
pub enum InvalidJobProgress {
    #[error("job progress total must be greater than zero")]
    ZeroTotal,
    #[error("completed amount {completed} exceeds total {total}")]
    CompletedExceedsTotal { completed: u64, total: u64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobFailureKind {
    Io,
    PermissionDenied,
    Conflict,
    Unsupported,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobFailure {
    kind: JobFailureKind,
    message: String,
}

impl JobFailure {
    pub fn new(kind: JobFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub const fn kind(&self) -> JobFailureKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl JobState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobCommand {
    Start,
    Pause,
    Resume,
    SetProgress(JobProgress),
    Complete,
    Cancel,
    Fail(JobFailure),
}

impl JobCommand {
    pub const fn kind(&self) -> JobCommandKind {
        match self {
            Self::Start => JobCommandKind::Start,
            Self::Pause => JobCommandKind::Pause,
            Self::Resume => JobCommandKind::Resume,
            Self::SetProgress(_) => JobCommandKind::SetProgress,
            Self::Complete => JobCommandKind::Complete,
            Self::Cancel => JobCommandKind::Cancel,
            Self::Fail(_) => JobCommandKind::Fail,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobCommandKind {
    Start,
    Pause,
    Resume,
    SetProgress,
    Complete,
    Cancel,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobEventKind {
    Queued,
    Started,
    Paused,
    Resumed,
    Progressed(JobProgress),
    Completed,
    Cancelled,
    Failed(JobFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobEvent {
    operation_id: OperationId,
    job_id: JobId,
    previous_state: Option<JobState>,
    state: JobState,
    kind: JobEventKind,
}

impl JobEvent {
    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    pub const fn previous_state(&self) -> Option<JobState> {
        self.previous_state
    }

    pub const fn state(&self) -> JobState {
        self.state
    }

    pub const fn kind(&self) -> &JobEventKind {
        &self.kind
    }
}

#[derive(Clone, Debug)]
pub struct JobRecord {
    operation_id: OperationId,
    job_id: JobId,
    state: JobState,
    progress: JobProgress,
    failure: Option<JobFailure>,
}

impl JobRecord {
    pub const fn new(operation_id: OperationId, job_id: JobId) -> Self {
        Self {
            operation_id,
            job_id,
            state: JobState::Queued,
            progress: JobProgress::indeterminate(),
            failure: None,
        }
    }

    pub const fn operation_id(&self) -> OperationId {
        self.operation_id
    }

    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    pub const fn state(&self) -> JobState {
        self.state
    }

    pub const fn progress(&self) -> JobProgress {
        self.progress
    }

    pub fn failure(&self) -> Option<&JobFailure> {
        self.failure.as_ref()
    }

    pub fn queued_event(&self) -> JobEvent {
        JobEvent {
            operation_id: self.operation_id,
            job_id: self.job_id,
            previous_state: None,
            state: JobState::Queued,
            kind: JobEventKind::Queued,
        }
    }

    pub fn apply(&mut self, command: JobCommand) -> Result<JobEvent, JobTransitionError> {
        let command_kind = command.kind();
        let previous_state = self.state;
        let (state, kind) = match (previous_state, command) {
            (JobState::Queued, JobCommand::Start) => (JobState::Running, JobEventKind::Started),
            (JobState::Queued | JobState::Running | JobState::Paused, JobCommand::Cancel) => {
                (JobState::Cancelled, JobEventKind::Cancelled)
            }
            (
                JobState::Queued | JobState::Running | JobState::Paused,
                JobCommand::Fail(failure),
            ) => {
                self.failure = Some(failure.clone());
                (JobState::Failed, JobEventKind::Failed(failure))
            }
            (JobState::Running, JobCommand::Pause) => (JobState::Paused, JobEventKind::Paused),
            (JobState::Paused, JobCommand::Resume) => (JobState::Running, JobEventKind::Resumed),
            (JobState::Running, JobCommand::SetProgress(progress)) => {
                self.progress = progress;
                (JobState::Running, JobEventKind::Progressed(progress))
            }
            (JobState::Running, JobCommand::Complete) => {
                if let Some(total) = self.progress.total {
                    self.progress.completed = total.get();
                }
                (JobState::Completed, JobEventKind::Completed)
            }
            (state, _) => {
                return Err(JobTransitionError {
                    job_id: self.job_id,
                    state,
                    command: command_kind,
                });
            }
        };

        self.state = state;
        Ok(JobEvent {
            operation_id: self.operation_id,
            job_id: self.job_id,
            previous_state: Some(previous_state),
            state,
            kind,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Error)]
#[error("job {job_id:?} cannot apply {command:?} while in {state:?}")]
pub struct JobTransitionError {
    pub job_id: JobId,
    pub state: JobState,
    pub command: JobCommandKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> JobRecord {
        JobRecord::new(
            OperationId::new(NonZeroU64::MIN),
            JobId::new(NonZeroU64::MIN),
        )
    }

    #[test]
    fn running_job_supports_progress_pause_resume_and_completion() {
        let mut job = record();
        assert_eq!(job.queued_event().kind(), &JobEventKind::Queued);
        assert_eq!(
            job.apply(JobCommand::Start)
                .expect("queued job should start")
                .kind(),
            &JobEventKind::Started
        );

        let progress = JobProgress::new(5, Some(10)).expect("progress should be valid");
        job.apply(JobCommand::SetProgress(progress))
            .expect("running job should accept progress");
        assert_eq!(job.progress().fraction(), Some(0.5));
        job.apply(JobCommand::Pause)
            .expect("running job should pause");
        job.apply(JobCommand::Resume)
            .expect("paused job should resume");
        job.apply(JobCommand::Complete)
            .expect("running job should complete");
        assert_eq!(job.state(), JobState::Completed);
        assert_eq!(job.progress().fraction(), Some(1.0));
        assert!(job.state().is_terminal());
    }

    #[test]
    fn terminal_jobs_reject_every_follow_up_command() {
        let mut job = record();
        job.apply(JobCommand::Cancel)
            .expect("queued job should cancel");

        let error = job
            .apply(JobCommand::Start)
            .expect_err("cancelled job must remain terminal");
        assert_eq!(error.state, JobState::Cancelled);
        assert_eq!(error.command, JobCommandKind::Start);
    }

    #[test]
    fn failure_is_structured_and_retained() {
        let mut job = record();
        let failure = JobFailure::new(JobFailureKind::PermissionDenied, "access denied");
        let event = job
            .apply(JobCommand::Fail(failure.clone()))
            .expect("queued job may fail before starting");

        assert_eq!(event.kind(), &JobEventKind::Failed(failure.clone()));
        assert_eq!(job.failure(), Some(&failure));
        assert_eq!(job.state(), JobState::Failed);
    }

    #[test]
    fn pause_resume_and_progress_have_strict_states() {
        let mut job = record();
        assert!(job.apply(JobCommand::Pause).is_err());
        assert!(
            job.apply(JobCommand::SetProgress(JobProgress::indeterminate()))
                .is_err()
        );
        job.apply(JobCommand::Start)
            .expect("queued job should start");
        assert!(job.apply(JobCommand::Resume).is_err());
    }

    #[test]
    fn progress_rejects_zero_or_overrun_totals() {
        assert_eq!(
            JobProgress::new(0, Some(0)),
            Err(InvalidJobProgress::ZeroTotal)
        );
        assert_eq!(
            JobProgress::new(11, Some(10)),
            Err(InvalidJobProgress::CompletedExceedsTotal {
                completed: 11,
                total: 10
            })
        );
    }
}
