use std::{
    collections::{HashMap, VecDeque},
    num::NonZeroU64,
};

use floe_core::{
    JobCommand, JobEvent, JobId, JobRecord, JobState, JobTransitionError, OperationId,
};
use thiserror::Error;

/// Application-owned registry and event boundary for future filesystem jobs.
///
/// It deliberately executes no filesystem operations. A later bounded executor
/// will consume operation models while GTK only submits commands and observes
/// the events collected here.
#[derive(Debug)]
pub struct ApplicationJobManager {
    next_operation_id: u64,
    next_job_id: u64,
    jobs: HashMap<JobId, JobRecord>,
    events: VecDeque<JobEvent>,
}

impl ApplicationJobManager {
    pub fn new() -> Self {
        Self {
            next_operation_id: 1,
            next_job_id: 1,
            jobs: HashMap::new(),
            events: VecDeque::new(),
        }
    }

    pub fn queue_operation(&mut self) -> Result<JobEvent, JobManagerError> {
        let operation_id = OperationId::new(allocate_id(&mut self.next_operation_id)?);
        self.queue_attempt(operation_id)
    }

    pub fn transition(
        &mut self,
        job_id: JobId,
        command: JobCommand,
    ) -> Result<JobEvent, JobManagerError> {
        let record = self
            .jobs
            .get_mut(&job_id)
            .ok_or(JobManagerError::UnknownJob(job_id))?;
        let event = record.apply(command)?;
        self.events.push_back(event.clone());
        Ok(event)
    }

    pub fn retry(&mut self, job_id: JobId) -> Result<JobEvent, JobManagerError> {
        let record = self
            .jobs
            .get(&job_id)
            .ok_or(JobManagerError::UnknownJob(job_id))?;
        if !matches!(record.state(), JobState::Cancelled | JobState::Failed) {
            return Err(JobManagerError::RetryNotAllowed {
                job_id,
                state: record.state(),
            });
        }
        self.queue_attempt(record.operation_id())
    }

    pub fn record(&self, job_id: JobId) -> Option<&JobRecord> {
        self.jobs.get(&job_id)
    }

    pub fn drain_events(&mut self) -> Vec<JobEvent> {
        self.events.drain(..).collect()
    }

    fn queue_attempt(&mut self, operation_id: OperationId) -> Result<JobEvent, JobManagerError> {
        let job_id = JobId::new(allocate_id(&mut self.next_job_id)?);
        let record = JobRecord::new(operation_id, job_id);
        let event = record.queued_event();
        self.jobs.insert(job_id, record);
        self.events.push_back(event.clone());
        Ok(event)
    }
}

impl Default for ApplicationJobManager {
    fn default() -> Self {
        Self::new()
    }
}

fn allocate_id(counter: &mut u64) -> Result<NonZeroU64, JobManagerError> {
    let id = NonZeroU64::new(*counter).ok_or(JobManagerError::IdentifierExhausted)?;
    *counter = counter.checked_add(1).unwrap_or(0);
    Ok(id)
}

#[derive(Debug, Error)]
pub enum JobManagerError {
    #[error("job identifier space is exhausted")]
    IdentifierExhausted,
    #[error("job {0:?} is not registered")]
    UnknownJob(JobId),
    #[error("job {job_id:?} cannot be retried from {state:?}")]
    RetryNotAllowed { job_id: JobId, state: JobState },
    #[error(transparent)]
    InvalidTransition(#[from] JobTransitionError),
}

#[cfg(test)]
mod tests {
    use floe_core::{JobCommand, JobEventKind, JobFailure, JobFailureKind, JobState};

    use super::*;

    #[test]
    fn queue_transition_and_drain_form_an_observable_event_stream() {
        let mut manager = ApplicationJobManager::new();
        let queued = manager
            .queue_operation()
            .expect("identifier allocation should succeed");
        let started = manager
            .transition(queued.job_id(), JobCommand::Start)
            .expect("queued job should start");

        assert_eq!(queued.kind(), &JobEventKind::Queued);
        assert_eq!(started.kind(), &JobEventKind::Started);
        assert_eq!(
            manager.record(queued.job_id()).map(JobRecord::state),
            Some(JobState::Running)
        );
        assert_eq!(manager.drain_events(), vec![queued, started]);
        assert!(manager.drain_events().is_empty());
    }

    #[test]
    fn retry_keeps_operation_identity_and_allocates_a_new_job() {
        let mut manager = ApplicationJobManager::new();
        let queued = manager
            .queue_operation()
            .expect("identifier allocation should succeed");
        manager
            .transition(
                queued.job_id(),
                JobCommand::Fail(JobFailure::new(JobFailureKind::Io, "fixture failure")),
            )
            .expect("queued job may fail");

        let retry = manager
            .retry(queued.job_id())
            .expect("failed job should create retry attempt");
        assert_eq!(retry.operation_id(), queued.operation_id());
        assert_ne!(retry.job_id(), queued.job_id());
        assert_eq!(retry.state(), JobState::Queued);
    }

    #[test]
    fn retry_and_transition_reject_invalid_job_states() {
        let mut manager = ApplicationJobManager::new();
        let queued = manager
            .queue_operation()
            .expect("identifier allocation should succeed");
        assert!(matches!(
            manager.retry(queued.job_id()),
            Err(JobManagerError::RetryNotAllowed { .. })
        ));

        manager
            .transition(queued.job_id(), JobCommand::Cancel)
            .expect("queued job should cancel");
        assert!(
            manager
                .transition(queued.job_id(), JobCommand::Start)
                .is_err()
        );
    }
}
