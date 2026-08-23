use std::sync::{Arc, Mutex};

use crate::{
    copy_executor::{CopyExecutor, CopyExecutorSpawnError, SharedJobManager},
    job_manager::ApplicationJobManager,
};

/// Application-wide services and state that outlive any one browser concern.
#[derive(Debug)]
pub struct ApplicationState {
    pub jobs: SharedJobManager,
    copy_executor: CopyExecutor,
}

impl ApplicationState {
    pub fn new() -> Result<Self, CopyExecutorSpawnError> {
        let jobs = Arc::new(Mutex::new(ApplicationJobManager::new()));
        let copy_executor = CopyExecutor::spawn(Arc::clone(&jobs))?;
        Ok(Self {
            jobs,
            copy_executor,
        })
    }

    pub fn copy_executor(&self) -> &CopyExecutor {
        &self.copy_executor
    }
}
