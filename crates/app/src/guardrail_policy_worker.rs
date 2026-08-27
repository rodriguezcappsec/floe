//! Capacity-one persistence worker for Protected Folder policy changes.

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread::{self, JoinHandle},
};

use floe_core::ProtectedRoots;

use crate::guardrail_store::GuardrailStore;

pub const GUARDRAIL_POLICY_QUEUE_CAPACITY: usize = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GuardrailPolicyRequest {
    Persist(ProtectedRoots),
    ResetBlocked(ProtectedRoots),
}

impl GuardrailPolicyRequest {
    pub fn policy(&self) -> &ProtectedRoots {
        match self {
            Self::Persist(policy) | Self::ResetBlocked(policy) => policy,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailPolicyResponse {
    request_id: u64,
    request: GuardrailPolicyRequest,
    result: Result<(), String>,
}

impl GuardrailPolicyResponse {
    pub const fn request_id(&self) -> u64 {
        self.request_id
    }

    pub fn request(&self) -> &GuardrailPolicyRequest {
        &self.request
    }

    pub fn result(&self) -> &Result<(), String> {
        &self.result
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GuardrailPolicySubmitError {
    #[error("another Protected Folder policy save is already queued")]
    QueueFull,
    #[error("Protected Folder policy worker stopped")]
    WorkerStopped,
}

enum WorkerMessage {
    Persist {
        request_id: u64,
        request: GuardrailPolicyRequest,
    },
    Shutdown,
}

#[derive(Debug)]
pub struct GuardrailPolicyWorker {
    sender: SyncSender<WorkerMessage>,
    receiver: Receiver<GuardrailPolicyResponse>,
    join: Option<JoinHandle<()>>,
    next_request_id: u64,
}

impl GuardrailPolicyWorker {
    pub fn spawn(store_path: PathBuf) -> Result<Self, std::io::Error> {
        let (sender, worker_receiver) = mpsc::sync_channel(GUARDRAIL_POLICY_QUEUE_CAPACITY);
        let (response_sender, receiver) = mpsc::sync_channel(GUARDRAIL_POLICY_QUEUE_CAPACITY);
        let join = thread::Builder::new()
            .name("floe-guardrail-policy".to_owned())
            .spawn(move || {
                while let Ok(message) = worker_receiver.recv() {
                    match message {
                        WorkerMessage::Persist {
                            request_id,
                            request,
                        } => {
                            let result = GuardrailStore::persist(&store_path, request.policy())
                                .map_err(|error| error.to_string());
                            if response_sender
                                .send(GuardrailPolicyResponse {
                                    request_id,
                                    request,
                                    result,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        WorkerMessage::Shutdown => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            receiver,
            join: Some(join),
            next_request_id: 1,
        })
    }

    pub fn submit(
        &mut self,
        request: GuardrailPolicyRequest,
    ) -> Result<u64, GuardrailPolicySubmitError> {
        let request_id = self.next_request_id;
        match self.sender.try_send(WorkerMessage::Persist {
            request_id,
            request,
        }) {
            Ok(()) => {
                self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
                Ok(request_id)
            }
            Err(TrySendError::Full(_)) => Err(GuardrailPolicySubmitError::QueueFull),
            Err(TrySendError::Disconnected(_)) => Err(GuardrailPolicySubmitError::WorkerStopped),
        }
    }

    pub fn try_response(&self) -> Option<GuardrailPolicyResponse> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for GuardrailPolicyWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(WorkerMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
