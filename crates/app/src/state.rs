use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use floe_core::{ConflictPolicy, CopyRequest, JobEvent, JobId, SymlinkPolicy};
use thiserror::Error;

use crate::{
    copy_executor::{
        CopyCancelError, CopyExecutor, CopyExecutorSpawnError, CopySubmission, CopySubmitError,
        SharedJobManager,
    },
    job_manager::ApplicationJobManager,
};

/// Application-owned copy buffer. It retains the original Linux path and is
/// deliberately independent of rendered or lossy filename text.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CopyBuffer {
    source: Option<PathBuf>,
}

impl CopyBuffer {
    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    fn stage(&mut self, source: PathBuf) -> Result<(), CopyInteractionError> {
        if source.file_name().is_none() {
            return Err(CopyInteractionError::InvalidSource(source));
        }
        self.source = Some(source);
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CopyInteractionError {
    #[error("select an item to copy first")]
    EmptyBuffer,
    #[error("this path cannot be copied: {}", .0.display())]
    InvalidSource(PathBuf),
    #[error("open a destination outside the copied folder, then paste again")]
    DestinationInsideSource,
    #[error(transparent)]
    Submit(#[from] CopySubmitError),
    #[error(transparent)]
    Cancel(#[from] CopyCancelError),
}

/// Application-wide services and state that outlive any one browser concern.
#[derive(Debug)]
pub struct ApplicationState {
    pub jobs: SharedJobManager,
    copy_executor: CopyExecutor,
    copy_buffer: RefCell<CopyBuffer>,
    copy_requests: RefCell<HashMap<JobId, CopyRequest>>,
}

impl ApplicationState {
    pub fn new() -> Result<Self, CopyExecutorSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        Ok(Self {
            jobs,
            copy_executor,
            copy_buffer: RefCell::new(CopyBuffer::default()),
            copy_requests: RefCell::new(HashMap::new()),
        })
    }

    pub fn stage_copy(&self, source: PathBuf) -> Result<(), CopyInteractionError> {
        self.copy_buffer.borrow_mut().stage(source)
    }

    pub fn staged_copy(&self) -> Option<PathBuf> {
        self.copy_buffer.borrow().source().map(Path::to_path_buf)
    }

    pub fn submit_paste(
        &self,
        destination_directory: &Path,
    ) -> Result<CopySubmission, CopyInteractionError> {
        let request = self.paste_request(destination_directory)?;
        match self.copy_executor.submit_copy(request.clone()) {
            Ok(submission) => {
                self.copy_requests
                    .borrow_mut()
                    .insert(submission.job_id(), request);
                Ok(submission)
            }
            Err(error) => {
                if let Some(job_id) = error.job_id() {
                    self.copy_requests.borrow_mut().insert(job_id, request);
                }
                Err(error.into())
            }
        }
    }

    pub fn copy_request(&self, job_id: JobId) -> Option<CopyRequest> {
        self.copy_requests.borrow().get(&job_id).cloned()
    }

    pub fn finish_copy(&self, job_id: JobId) -> Option<CopyRequest> {
        self.copy_requests.borrow_mut().remove(&job_id)
    }

    pub fn cancel_copy(&self, job_id: JobId) -> Result<(), CopyInteractionError> {
        self.copy_executor.cancel(job_id)?;
        Ok(())
    }

    pub fn drain_job_events(&self) -> Vec<JobEvent> {
        lock(&self.jobs).drain_events()
    }

    fn paste_request(
        &self,
        destination_directory: &Path,
    ) -> Result<CopyRequest, CopyInteractionError> {
        let source = self
            .staged_copy()
            .ok_or(CopyInteractionError::EmptyBuffer)?;
        let name = source
            .file_name()
            .ok_or_else(|| CopyInteractionError::InvalidSource(source.clone()))?;
        if lexically_normalized(destination_directory).starts_with(lexically_normalized(&source)) {
            return Err(CopyInteractionError::DestinationInsideSource);
        }
        let destination = destination_directory.join(name);
        Ok(CopyRequest::new(
            source,
            destination,
            ConflictPolicy::FailIfExists,
            SymlinkPolicy::Preserve,
        ))
    }

    #[cfg(test)]
    fn submit_paste_with_cancellation(
        &self,
        destination_directory: &Path,
        cancellation: floe_core::CopyCancellation,
    ) -> Result<CopySubmission, CopyInteractionError> {
        let request = self.paste_request(destination_directory)?;
        let submission = self
            .copy_executor
            .submit_copy_with_cancellation(request.clone(), cancellation)?;
        self.copy_requests
            .borrow_mut()
            .insert(submission.job_id(), request);
        Ok(submission)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn lexically_normalized(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if normalized.file_name().is_some() => {
                normalized.pop();
            }
            Component::ParentDir if !path.has_root() => normalized.push(component.as_os_str()),
            Component::ParentDir => {}
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::ffi::{OsStrExt, OsStringExt},
        thread,
        time::{Duration, Instant},
    };

    use floe_core::{CopyCancellation, JobEventKind, JobFailureKind, JobState};
    use tempfile::tempdir;

    use super::*;

    fn wait_for_terminal(state: &ApplicationState, job_id: JobId) -> JobState {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(job_state) = lock(&state.jobs)
                .record(job_id)
                .map(|record| record.state())
                && job_state.is_terminal()
            {
                return job_state;
            }
            assert!(
                Instant::now() < deadline,
                "copy job did not become terminal"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn copy_interaction_stages_original_path_and_builds_exact_destination() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let name = OsString::from_vec(b"copy-\xff".to_vec());
        let source = source_directory.join(&name);
        fs::write(&source, b"floe").expect("source fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");

        state
            .stage_copy(source.clone())
            .expect("source should be staged");
        let submission = state
            .submit_paste(&destination_directory)
            .expect("paste should be submitted");

        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Completed
        );
        let copied = fs::read_dir(&destination_directory)
            .expect("destination should be readable")
            .next()
            .expect("destination should contain copied item")
            .expect("copied entry should be readable");
        assert_eq!(copied.file_name().as_bytes(), name.as_bytes());
        assert_eq!(
            fs::read(copied.path()).expect("copy should be readable"),
            b"floe"
        );
    }

    #[test]
    fn copy_interaction_rejects_paste_without_staged_source() {
        let fixture = tempdir().expect("temporary directory should be available");
        let state = ApplicationState::new().expect("application state should start");

        let error = state
            .submit_paste(fixture.path())
            .expect_err("empty copy buffer must be rejected");

        assert!(matches!(error, CopyInteractionError::EmptyBuffer));
    }

    #[test]
    fn copy_interaction_rejects_destination_inside_staged_folder() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source = fixture.path().join("source");
        let nested_destination = source.join("nested");
        fs::create_dir(&source).expect("source directory should be creatable");
        fs::create_dir(&nested_destination).expect("nested directory should be creatable");
        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source).expect("source should be staged");

        let error = state
            .submit_paste(&nested_destination)
            .expect_err("paste into copied folder must be rejected");

        assert!(matches!(
            error,
            CopyInteractionError::DestinationInsideSource
        ));
    }

    #[test]
    fn copy_interaction_surfaces_conflict_failure_event() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let source = source_directory.join("item");
        fs::write(&source, b"new").expect("source fixture should be writable");
        fs::write(destination_directory.join("item"), b"keep")
            .expect("conflict fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");
        state.stage_copy(source).expect("source should be staged");

        let submission = state
            .submit_paste(&destination_directory)
            .expect("conflicting paste should still be submitted");

        assert_eq!(
            wait_for_terminal(&state, submission.job_id()),
            JobState::Failed
        );
        let events = state.drain_job_events();
        assert!(events.iter().any(|event| {
            event.job_id() == submission.job_id()
                && matches!(
                    event.kind(),
                    JobEventKind::Failed(failure)
                        if failure.kind() == JobFailureKind::Conflict
                )
        }));
    }

    #[test]
    fn copy_interaction_maps_cancellation_and_success_lifecycle() {
        let fixture = tempdir().expect("temporary directory should be available");
        let source_directory = fixture.path().join("source");
        let destination_directory = fixture.path().join("destination");
        fs::create_dir(&source_directory).expect("source directory should be creatable");
        fs::create_dir(&destination_directory).expect("destination directory should be creatable");
        let source = source_directory.join("item");
        fs::write(&source, b"content").expect("source fixture should be writable");
        let state = ApplicationState::new().expect("application state should start");
        state
            .stage_copy(source.clone())
            .expect("source should be staged");
        let cancellation = CopyCancellation::new();
        cancellation.cancel();

        let cancelled = state
            .submit_paste_with_cancellation(&destination_directory, cancellation)
            .expect("cancelled paste should be submitted");
        assert_eq!(
            wait_for_terminal(&state, cancelled.job_id()),
            JobState::Cancelled
        );

        let completed_directory = fixture.path().join("completed");
        fs::create_dir(&completed_directory).expect("completion directory should be creatable");
        let completed = state
            .submit_paste(&completed_directory)
            .expect("second paste should be submitted");
        assert_eq!(
            wait_for_terminal(&state, completed.job_id()),
            JobState::Completed
        );

        let events = state.drain_job_events();
        assert!(events.iter().any(|event| {
            event.job_id() == cancelled.job_id() && event.kind() == &JobEventKind::Cancelled
        }));
        assert!(events.iter().any(|event| {
            event.job_id() == completed.job_id() && event.kind() == &JobEventKind::Completed
        }));
    }
}
