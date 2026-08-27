//! Application-layer coordinator for Phase 18X guardrail policy and preflight.
//!
//! The controller is GTK-independent. It keeps corrupt storage distinct from a
//! missing policy, aggregates a bounded sequence of exact destructive scopes,
//! and issues single-use permits only after a complete safe preflight or an
//! explicit confirmation of every reported risk.

use std::{
    collections::VecDeque,
    fmt, io,
    path::{Path, PathBuf},
};

use floe_core::{
    DestructiveScope, GUARDRAIL_ACTIVE_PERMIT_CAPACITY, GuardrailPermit, GuardrailPermitError,
    GuardrailPermitIssuer, PreflightDecision, PreflightRisk, ProtectedRoots,
};
use thiserror::Error;

use crate::{
    guardrail_preflight::{
        GuardrailPreflightError, GuardrailPreflightOutcome, GuardrailPreflightSubmission,
        GuardrailPreflightSubmitError, GuardrailPreflightWorker, PreflightEnvironment,
    },
    guardrail_store::{GuardrailPolicyLoad, GuardrailStore, GuardrailStoreError},
};

pub const GUARDRAIL_REVIEW_SCOPE_CAPACITY: usize = GUARDRAIL_ACTIVE_PERMIT_CAPACITY;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardrailStoreHealth {
    Missing,
    Ready,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailReviewRequest {
    scopes: Vec<DestructiveScope>,
    environment: PreflightEnvironment,
}

impl GuardrailReviewRequest {
    pub fn new(
        scopes: Vec<DestructiveScope>,
        environment: PreflightEnvironment,
    ) -> Result<Self, GuardrailControllerError> {
        if scopes.is_empty() {
            return Err(GuardrailControllerError::EmptyScope);
        }
        if scopes.len() > GUARDRAIL_REVIEW_SCOPE_CAPACITY {
            return Err(GuardrailControllerError::ScopeCapacityExceeded {
                count: scopes.len(),
                capacity: GUARDRAIL_REVIEW_SCOPE_CAPACITY,
            });
        }
        Ok(Self {
            scopes,
            environment,
        })
    }

    pub fn scopes(&self) -> &[DestructiveScope] {
        &self.scopes
    }

    pub fn environment(&self) -> &PreflightEnvironment {
        &self.environment
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuardrailReviewSubmission {
    generation: u64,
}

impl GuardrailReviewSubmission {
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailReview {
    generation: u64,
    policy_generation: u64,
    outcomes: Vec<GuardrailPreflightOutcome>,
    risks: Vec<PreflightRisk>,
}

impl GuardrailReview {
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub const fn policy_generation(&self) -> u64 {
        self.policy_generation
    }

    pub fn outcomes(&self) -> &[GuardrailPreflightOutcome] {
        &self.outcomes
    }

    pub fn scopes(&self) -> impl ExactSizeIterator<Item = &DestructiveScope> {
        self.outcomes.iter().map(GuardrailPreflightOutcome::scope)
    }

    pub fn risks(&self) -> &[PreflightRisk] {
        &self.risks
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailAuthorizationItem {
    permit: GuardrailPermit,
    scope: DestructiveScope,
}

impl GuardrailAuthorizationItem {
    pub fn scope(&self) -> &DestructiveScope {
        &self.scope
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardrailAuthorization {
    review_generation: u64,
    policy_generation: u64,
    items: Vec<GuardrailAuthorizationItem>,
}

impl GuardrailAuthorization {
    pub const fn review_generation(&self) -> u64 {
        self.review_generation
    }

    pub const fn policy_generation(&self) -> u64 {
        self.policy_generation
    }

    pub fn items(&self) -> &[GuardrailAuthorizationItem] {
        &self.items
    }

    pub fn into_items(self) -> Vec<GuardrailAuthorizationItem> {
        self.items
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardrailConfirmation {
    Confirm,
    Deny,
}

#[derive(Debug)]
pub enum GuardrailPoll {
    Pending,
    ReviewRequired(GuardrailReview),
    Allowed(GuardrailAuthorization),
    Cancelled,
    Blocked(GuardrailBlock),
}

#[derive(Debug)]
pub enum GuardrailResolution {
    Allowed(GuardrailAuthorization),
    Denied,
}

#[derive(Debug, Error)]
pub enum GuardrailBlock {
    #[error(
        "destructive operations are blocked until the guardrail policy store error is acknowledged"
    )]
    Store,
    #[error("guardrail preflight failed")]
    Preflight(#[source] GuardrailPreflightError),
}

#[derive(Debug, Error)]
pub enum GuardrailControllerError {
    #[error("guardrail review requires at least one destructive scope")]
    EmptyScope,
    #[error("guardrail review scope count {count} exceeds capacity {capacity}")]
    ScopeCapacityExceeded { count: usize, capacity: usize },
    #[error("guardrail policy store is blocked")]
    StoreBlocked,
    #[error("another guardrail review is active")]
    Busy,
    #[error("guardrail review generation {0} is not awaiting confirmation")]
    ReviewNotFound(u64),
    #[error("guardrail review generation is exhausted")]
    GenerationExhausted,
    #[error("guardrail authorization could not be issued")]
    Permit(#[from] GuardrailPermitError),
    #[error("guardrail preflight could not be submitted")]
    Submit(#[from] GuardrailPreflightSubmitError),
    #[error("guardrail policy store could not be saved")]
    Store(#[from] GuardrailStoreError),
    #[error("could not start guardrail preflight worker")]
    Worker(#[source] io::Error),
}

struct ActiveScan {
    generation: u64,
    policy_generation: u64,
    policy: ProtectedRoots,
    environment: PreflightEnvironment,
    remaining: VecDeque<DestructiveScope>,
    current_scope: DestructiveScope,
    current_submission: GuardrailPreflightSubmission,
    outcomes: Vec<GuardrailPreflightOutcome>,
}

pub struct GuardrailController {
    store_path: PathBuf,
    store_health: GuardrailStoreHealth,
    store_error: Option<GuardrailStoreError>,
    policy: ProtectedRoots,
    worker: GuardrailPreflightWorker,
    permits: GuardrailPermitIssuer,
    outstanding_permits: usize,
    next_generation: u64,
    active: Option<ActiveScan>,
    awaiting_review: Option<GuardrailReview>,
    cancelled_generation: Option<u64>,
}

impl fmt::Debug for GuardrailController {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GuardrailController")
            .field("store_path", &self.store_path)
            .field("store_health", &self.store_health)
            .field("policy_generation", &self.policy.generation())
            .field("protected_root_count", &self.policy.roots().len())
            .field("outstanding_permits", &self.outstanding_permits)
            .field(
                "active_review",
                &self.active.as_ref().map(|scan| scan.generation),
            )
            .finish_non_exhaustive()
    }
}

impl GuardrailController {
    pub fn load_at(store_path: PathBuf) -> Result<Self, GuardrailControllerError> {
        let load = GuardrailStore::load_fail_closed(&store_path);
        let (store_health, store_error, policy) = match load {
            GuardrailPolicyLoad::Missing => (
                GuardrailStoreHealth::Missing,
                None,
                ProtectedRoots::default(),
            ),
            GuardrailPolicyLoad::Ready(policy) => (GuardrailStoreHealth::Ready, None, policy),
            GuardrailPolicyLoad::Blocked(error) => (
                GuardrailStoreHealth::Blocked,
                Some(error),
                ProtectedRoots::default(),
            ),
        };
        let worker = GuardrailPreflightWorker::spawn().map_err(|error| {
            GuardrailControllerError::Worker(io::Error::other(error.to_string()))
        })?;
        Ok(Self {
            store_path,
            store_health,
            store_error,
            policy,
            worker,
            permits: GuardrailPermitIssuer::default(),
            outstanding_permits: 0,
            next_generation: 0,
            active: None,
            awaiting_review: None,
            cancelled_generation: None,
        })
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn authorize_deterministic_scope_for_test(
        &mut self,
        scope: DestructiveScope,
    ) -> Result<GuardrailAuthorizationItem, GuardrailControllerError> {
        if self.store_health == GuardrailStoreHealth::Blocked {
            return Err(GuardrailControllerError::StoreBlocked);
        }
        let generation = self.next_generation;
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(GuardrailControllerError::GenerationExhausted)?;
        let outcome =
            GuardrailPreflightOutcome::deterministic_for_test(scope, self.policy.generation());
        let risks = match outcome.decision() {
            PreflightDecision::Proceed => Vec::new(),
            PreflightDecision::Confirm(risks) => risks.clone(),
        };
        let review = GuardrailReview {
            generation,
            policy_generation: self.policy.generation(),
            outcomes: vec![outcome],
            risks,
        };
        self.issue_authorization(review)?
            .into_items()
            .into_iter()
            .next()
            .ok_or(GuardrailControllerError::EmptyScope)
    }

    pub const fn store_health(&self) -> GuardrailStoreHealth {
        self.store_health
    }

    pub fn store_error(&self) -> Option<&GuardrailStoreError> {
        self.store_error.as_ref()
    }

    pub fn store_path(&self) -> &Path {
        &self.store_path
    }

    pub fn policy(&self) -> &ProtectedRoots {
        &self.policy
    }

    pub fn begin_review(
        &mut self,
        request: GuardrailReviewRequest,
    ) -> Result<GuardrailReviewSubmission, GuardrailControllerError> {
        if self.store_health == GuardrailStoreHealth::Blocked {
            return Err(GuardrailControllerError::StoreBlocked);
        }
        if self.active.is_some() || self.awaiting_review.is_some() {
            return Err(GuardrailControllerError::Busy);
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(GuardrailControllerError::GenerationExhausted)?;
        let generation = self.next_generation;
        self.cancelled_generation = None;
        let mut remaining = VecDeque::from(request.scopes);
        let current_scope = remaining
            .pop_front()
            .ok_or(GuardrailControllerError::EmptyScope)?;
        let current_submission = self.worker.submit(
            current_scope.clone(),
            self.policy.clone(),
            request.environment.clone(),
        )?;
        self.active = Some(ActiveScan {
            generation,
            policy_generation: self.policy.generation(),
            policy: self.policy.clone(),
            environment: request.environment,
            remaining,
            current_scope,
            current_submission,
            outcomes: Vec::new(),
        });
        Ok(GuardrailReviewSubmission { generation })
    }

    pub fn poll(&mut self, generation: u64) -> Result<GuardrailPoll, GuardrailControllerError> {
        if self.cancelled_generation == Some(generation) && self.active.is_none() {
            self.cancelled_generation = None;
            return Ok(GuardrailPoll::Cancelled);
        }
        let Some(active) = self.active.as_mut() else {
            return Err(GuardrailControllerError::ReviewNotFound(generation));
        };
        if active.generation != generation {
            return Err(GuardrailControllerError::ReviewNotFound(generation));
        }
        let Some(result) = self
            .worker
            .take_result(active.current_submission.generation())
        else {
            return Ok(GuardrailPoll::Pending);
        };
        if self.cancelled_generation == Some(generation) {
            self.cancelled_generation = None;
            self.active = None;
            return Ok(GuardrailPoll::Cancelled);
        }
        match result.outcome {
            Ok(outcome) => active.outcomes.push(outcome),
            Err(GuardrailPreflightError::Cancelled) => {
                self.active = None;
                return Ok(GuardrailPoll::Cancelled);
            }
            Err(error) => {
                self.active = None;
                return Ok(GuardrailPoll::Blocked(GuardrailBlock::Preflight(error)));
            }
        }

        if let Some(next_scope) = active.remaining.pop_front() {
            let submission = self.worker.submit(
                next_scope.clone(),
                active.policy.clone(),
                active.environment.clone(),
            )?;
            active.current_scope = next_scope;
            active.current_submission = submission;
            return Ok(GuardrailPoll::Pending);
        }

        let active = self
            .active
            .take()
            .ok_or(GuardrailControllerError::ReviewNotFound(generation))?;
        let review = build_review(active);
        if review.risks.is_empty() {
            self.issue_authorization(review).map(GuardrailPoll::Allowed)
        } else {
            self.awaiting_review = Some(review.clone());
            Ok(GuardrailPoll::ReviewRequired(review))
        }
    }

    pub fn resolve_review(
        &mut self,
        generation: u64,
        confirmation: GuardrailConfirmation,
    ) -> Result<GuardrailResolution, GuardrailControllerError> {
        let Some(review) = self.awaiting_review.take() else {
            return Err(GuardrailControllerError::ReviewNotFound(generation));
        };
        if review.generation != generation {
            self.awaiting_review = Some(review);
            return Err(GuardrailControllerError::ReviewNotFound(generation));
        }
        match confirmation {
            GuardrailConfirmation::Deny => Ok(GuardrailResolution::Denied),
            GuardrailConfirmation::Confirm => self
                .issue_authorization(review)
                .map(GuardrailResolution::Allowed),
        }
    }

    pub fn cancel(&mut self, generation: u64) -> Result<(), GuardrailControllerError> {
        if self
            .active
            .as_ref()
            .is_some_and(|active| active.generation == generation)
        {
            self.worker.cancel_active();
            self.cancelled_generation = Some(generation);
            return Ok(());
        }
        if self
            .awaiting_review
            .as_ref()
            .is_some_and(|review| review.generation == generation)
        {
            self.awaiting_review = None;
            self.cancelled_generation = Some(generation);
            return Ok(());
        }
        Err(GuardrailControllerError::ReviewNotFound(generation))
    }

    pub fn consume_authorization(
        &mut self,
        authorization: GuardrailAuthorizationItem,
        exact_scope: &DestructiveScope,
    ) -> Result<(), GuardrailPermitError> {
        let result =
            self.permits
                .consume(authorization.permit, self.policy.generation(), exact_scope);
        self.outstanding_permits = self.outstanding_permits.saturating_sub(1);
        result
    }

    pub fn discard_authorization(
        &mut self,
        authorization: GuardrailAuthorizationItem,
    ) -> Result<(), GuardrailPermitError> {
        let exact_scope = authorization.scope.clone();
        self.consume_authorization(authorization, &exact_scope)
    }

    /// Install policy only after a separate bounded store worker has
    /// successfully persisted it. Changing generation revokes outstanding
    /// authorizations and cancels any in-flight review.
    pub fn install_persisted_policy(
        &mut self,
        policy: ProtectedRoots,
    ) -> Result<(), GuardrailControllerError> {
        if policy.generation() <= self.policy.generation() {
            return Err(GuardrailControllerError::GenerationExhausted);
        }
        self.worker.cancel_active();
        self.active = None;
        self.awaiting_review = None;
        self.permits.revoke_all();
        self.outstanding_permits = 0;
        self.policy = policy;
        self.store_health = GuardrailStoreHealth::Ready;
        self.store_error = None;
        Ok(())
    }

    /// Replace a blocked store only after an explicit user acknowledgement.
    /// This helper is synchronous for startup/recovery wiring; GTK callbacks
    /// must invoke equivalent persistence on an application-owned worker.
    pub fn acknowledge_and_reset_blocked_store(
        &mut self,
        acknowledged: bool,
    ) -> Result<(), GuardrailControllerError> {
        if self.store_health != GuardrailStoreHealth::Blocked || !acknowledged {
            return Err(GuardrailControllerError::StoreBlocked);
        }
        let policy = ProtectedRoots::with_generation(1, Vec::new())
            .map_err(|_| GuardrailControllerError::GenerationExhausted)?;
        GuardrailStore::persist(&self.store_path, &policy)?;
        self.policy = policy;
        self.store_health = GuardrailStoreHealth::Ready;
        self.store_error = None;
        self.permits.revoke_all();
        self.outstanding_permits = 0;
        Ok(())
    }

    fn issue_authorization(
        &mut self,
        review: GuardrailReview,
    ) -> Result<GuardrailAuthorization, GuardrailControllerError> {
        if review.policy_generation != self.policy.generation() {
            return Err(GuardrailControllerError::Permit(
                GuardrailPermitError::StalePolicy {
                    expected: review.policy_generation,
                    actual: self.policy.generation(),
                },
            ));
        }
        if review.outcomes.len()
            > GUARDRAIL_ACTIVE_PERMIT_CAPACITY.saturating_sub(self.outstanding_permits)
        {
            return Err(GuardrailControllerError::Permit(
                GuardrailPermitError::CapacityExceeded,
            ));
        }
        let mut items = Vec::with_capacity(review.outcomes.len());
        for outcome in review.outcomes {
            let scope = outcome.scope().clone();
            let permit = self
                .permits
                .issue(review.policy_generation, scope.clone())?;
            items.push(GuardrailAuthorizationItem { permit, scope });
        }
        self.outstanding_permits = self.outstanding_permits.saturating_add(items.len());
        Ok(GuardrailAuthorization {
            review_generation: review.generation,
            policy_generation: review.policy_generation,
            items,
        })
    }
}

fn build_review(active: ActiveScan) -> GuardrailReview {
    let mut risks = active
        .outcomes
        .iter()
        .flat_map(|outcome| match outcome.decision() {
            PreflightDecision::Proceed => Vec::new(),
            PreflightDecision::Confirm(risks) => risks.clone(),
        })
        .collect::<Vec<_>>();
    risks.sort_by_key(|risk| risk_order(*risk));
    risks.dedup();
    GuardrailReview {
        generation: active.generation,
        policy_generation: active.policy_generation,
        outcomes: active.outcomes,
        risks,
    }
}

const fn risk_order(risk: PreflightRisk) -> u8 {
    match risk {
        PreflightRisk::ProtectedPath => 0,
        PreflightRisk::IrreversibleAction => 1,
        PreflightRisk::LargeItemCount => 2,
        PreflightRisk::LargeByteCount => 3,
        PreflightRisk::DeepTree => 4,
        PreflightRisk::FilesystemRoot => 5,
        PreflightRisk::MountRoot => 6,
        PreflightRisk::HomeDirectory => 7,
        PreflightRisk::IncompleteScan => 8,
        PreflightRisk::ScanLimitExceeded => 9,
        PreflightRisk::ArithmeticOverflow => 10,
        PreflightRisk::UnknownFacts => 11,
    }
}
