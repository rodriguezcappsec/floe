//! GTK-independent data-loss guardrail policy.
//!
//! Protected roots are an accidental-change barrier. They are not access
//! control, encryption, sandboxing, or protection from another process. Path
//! matching is deliberately lexical and component-aware: it preserves exact
//! Unix path bytes and never resolves symbolic links or hard-link aliases.

use std::{
    collections::{HashMap, HashSet},
    path::{Component, Path, PathBuf},
};

use thiserror::Error;

pub const PROTECTED_ROOT_CAPACITY: usize = 128;
pub const GUARDRAIL_TARGET_CAPACITY: usize = 4_096;
pub const GUARDRAIL_PATH_BYTE_CAPACITY: usize = 4 * 1_024;
/// Maximum exact in-flight authorizations. This matches the existing bounded
/// batch-rename ceiling so a reviewed 4,096-item rename never needs to omit a
/// revised destination or authorize an item after it has already been queued.
pub const GUARDRAIL_ACTIVE_PERMIT_CAPACITY: usize = 4_096;

pub const LARGE_DESTRUCTIVE_ITEM_THRESHOLD: u64 = 1_000;
pub const LARGE_DESTRUCTIVE_BYTE_THRESHOLD: u64 = 10 * 1_024 * 1_024 * 1_024;
pub const LARGE_DESTRUCTIVE_DEPTH_THRESHOLD: u32 = 64;

pub const PREFLIGHT_ENTRY_CAPACITY: u64 = 250_000;
pub const PREFLIGHT_DIRECTORY_CAPACITY: u64 = 100_000;
pub const PREFLIGHT_DEPTH_CAPACITY: u32 = 256;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtectedRoots {
    generation: u64,
    roots: Vec<PathBuf>,
}

impl ProtectedRoots {
    pub fn new(roots: Vec<PathBuf>) -> Result<Self, ProtectedRootsError> {
        Self::with_generation(0, roots)
    }

    pub fn with_generation(
        generation: u64,
        roots: Vec<PathBuf>,
    ) -> Result<Self, ProtectedRootsError> {
        if roots.len() > PROTECTED_ROOT_CAPACITY {
            return Err(ProtectedRootsError::CapacityExceeded {
                count: roots.len(),
                capacity: PROTECTED_ROOT_CAPACITY,
            });
        }

        let mut validated = Vec::with_capacity(roots.len());
        let mut seen = HashSet::with_capacity(roots.len());
        for root in roots {
            validate_guardrail_path(&root)?;
            if !seen.insert(root.clone()) {
                return Err(ProtectedRootsError::Duplicate(root));
            }
            validated.push(root);
        }
        validated.sort_by(|left, right| left.as_os_str().cmp(right.as_os_str()));

        Ok(Self {
            generation,
            roots: validated,
        })
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    pub fn add(&mut self, root: PathBuf) -> Result<bool, ProtectedRootsError> {
        validate_guardrail_path(&root)?;
        match self
            .roots
            .binary_search_by(|candidate| candidate.as_os_str().cmp(root.as_os_str()))
        {
            Ok(_) => Ok(false),
            Err(index) => {
                if self.roots.len() == PROTECTED_ROOT_CAPACITY {
                    return Err(ProtectedRootsError::CapacityExceeded {
                        count: self.roots.len().saturating_add(1),
                        capacity: PROTECTED_ROOT_CAPACITY,
                    });
                }
                self.generation = next_generation(self.generation)?;
                self.roots.insert(index, root);
                Ok(true)
            }
        }
    }

    pub fn remove(&mut self, root: &Path) -> Result<bool, ProtectedRootsError> {
        validate_guardrail_path(root)?;
        let Ok(index) = self
            .roots
            .binary_search_by(|candidate| candidate.as_os_str().cmp(root.as_os_str()))
        else {
            return Ok(false);
        };
        self.generation = next_generation(self.generation)?;
        self.roots.remove(index);
        Ok(true)
    }

    /// Return every lexical protected-path intersection for `target`.
    ///
    /// The comparison uses path components, so `/one/two` never matches
    /// `/one/twenty`. It intentionally does not canonicalize, follow symlinks,
    /// or discover hard-link aliases; commit-time filesystem safety remains the
    /// responsibility of the underlying operation.
    pub fn intersections(
        &self,
        target: &Path,
    ) -> Result<Vec<ProtectedIntersection>, ProtectedRootsError> {
        validate_guardrail_path(target)?;
        let mut intersections = self
            .roots
            .iter()
            .filter_map(|root| {
                if target == root {
                    Some(ProtectedIntersection::new(
                        target.to_path_buf(),
                        root.clone(),
                        ProtectedRelation::Exact,
                    ))
                } else if target.starts_with(root) {
                    Some(ProtectedIntersection::new(
                        target.to_path_buf(),
                        root.clone(),
                        ProtectedRelation::Descendant,
                    ))
                } else if root.starts_with(target) {
                    Some(ProtectedIntersection::new(
                        target.to_path_buf(),
                        root.clone(),
                        ProtectedRelation::Ancestor,
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        intersections.sort_by(|left, right| {
            left.protected_root
                .as_os_str()
                .cmp(right.protected_root.as_os_str())
        });
        Ok(intersections)
    }

    pub fn intersects(&self, target: &Path) -> Result<bool, ProtectedRootsError> {
        Ok(!self.intersections(target)?.is_empty())
    }
}

fn next_generation(generation: u64) -> Result<u64, ProtectedRootsError> {
    generation
        .checked_add(1)
        .ok_or(ProtectedRootsError::GenerationExhausted)
}

pub fn validate_guardrail_path(path: &Path) -> Result<(), ProtectedRootsError> {
    if !path.is_absolute() {
        return Err(ProtectedRootsError::Relative(path.to_path_buf()));
    }
    if path.as_os_str().as_encoded_bytes().len() > GUARDRAIL_PATH_BYTE_CAPACITY {
        return Err(ProtectedRootsError::PathTooLong(path.to_path_buf()));
    }

    let mut saw_root = false;
    for component in path.components() {
        match component {
            Component::RootDir if !saw_root => saw_root = true,
            Component::Normal(_) if saw_root => {}
            Component::CurDir
            | Component::ParentDir
            | Component::Prefix(_)
            | Component::RootDir => {
                return Err(ProtectedRootsError::Unnormalized(path.to_path_buf()));
            }
            Component::Normal(_) => {
                return Err(ProtectedRootsError::Relative(path.to_path_buf()));
            }
        }
    }

    if !saw_root {
        return Err(ProtectedRootsError::Relative(path.to_path_buf()));
    }
    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        if let Component::Normal(component) = component {
            normalized.push(component);
        }
    }
    if normalized.as_os_str().as_encoded_bytes() != path.as_os_str().as_encoded_bytes() {
        return Err(ProtectedRootsError::Unnormalized(path.to_path_buf()));
    }
    Ok(())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProtectedRootsError {
    #[error("protected paths must be absolute: {}", .0.display())]
    Relative(PathBuf),
    #[error("protected paths must be lexically normalized: {}", .0.display())]
    Unnormalized(PathBuf),
    #[error("protected path exceeds the raw-byte limit: {}", .0.display())]
    PathTooLong(PathBuf),
    #[error("protected root occurs more than once: {}", .0.display())]
    Duplicate(PathBuf),
    #[error("protected-root count {count} exceeds capacity {capacity}")]
    CapacityExceeded { count: usize, capacity: usize },
    #[error("protected-root policy generation is exhausted")]
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProtectedRelation {
    Exact,
    Descendant,
    Ancestor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedIntersection {
    target: PathBuf,
    protected_root: PathBuf,
    relation: ProtectedRelation,
}

impl ProtectedIntersection {
    const fn new(target: PathBuf, protected_root: PathBuf, relation: ProtectedRelation) -> Self {
        Self {
            target,
            protected_root,
            relation,
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn protected_root(&self) -> &Path {
        &self.protected_root
    }

    pub const fn relation(&self) -> ProtectedRelation {
        self.relation
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DestructiveAction {
    Trash,
    PermanentDelete,
    Move,
    Rename,
    Overwrite,
}

impl DestructiveAction {
    pub const fn is_irreversible(self) -> bool {
        matches!(self, Self::PermanentDelete | Self::Overwrite)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreflightScanState {
    Complete,
    Incomplete,
    EntryLimitExceeded,
    DirectoryLimitExceeded,
    DepthLimitExceeded,
    ArithmeticOverflow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestructiveFacts {
    item_count: Option<u64>,
    byte_count: Option<u64>,
    max_depth: Option<u32>,
    scan_state: PreflightScanState,
    protected_intersections: Vec<ProtectedIntersection>,
    touches_filesystem_root: bool,
    touches_mount_root: bool,
    touches_home: bool,
}

impl DestructiveFacts {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        item_count: Option<u64>,
        byte_count: Option<u64>,
        max_depth: Option<u32>,
        scan_state: PreflightScanState,
        protected_intersections: Vec<ProtectedIntersection>,
        touches_filesystem_root: bool,
        touches_mount_root: bool,
        touches_home: bool,
    ) -> Self {
        Self {
            item_count,
            byte_count,
            max_depth,
            scan_state,
            protected_intersections,
            touches_filesystem_root,
            touches_mount_root,
            touches_home,
        }
    }

    pub const fn item_count(&self) -> Option<u64> {
        self.item_count
    }

    pub const fn byte_count(&self) -> Option<u64> {
        self.byte_count
    }

    pub const fn max_depth(&self) -> Option<u32> {
        self.max_depth
    }

    pub const fn scan_state(&self) -> PreflightScanState {
        self.scan_state
    }

    pub fn protected_intersections(&self) -> &[ProtectedIntersection] {
        &self.protected_intersections
    }

    pub const fn touches_filesystem_root(&self) -> bool {
        self.touches_filesystem_root
    }

    pub const fn touches_mount_root(&self) -> bool {
        self.touches_mount_root
    }

    pub const fn touches_home(&self) -> bool {
        self.touches_home
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PreflightRisk {
    ProtectedPath,
    IrreversibleAction,
    LargeItemCount,
    LargeByteCount,
    DeepTree,
    FilesystemRoot,
    MountRoot,
    HomeDirectory,
    IncompleteScan,
    ScanLimitExceeded,
    ArithmeticOverflow,
    UnknownFacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreflightDecision {
    Proceed,
    Confirm(Vec<PreflightRisk>),
}

impl PreflightDecision {
    pub fn evaluate(action: DestructiveAction, facts: &DestructiveFacts) -> Self {
        let mut risks = Vec::new();
        if !facts.protected_intersections.is_empty() {
            risks.push(PreflightRisk::ProtectedPath);
        }
        if action.is_irreversible() {
            risks.push(PreflightRisk::IrreversibleAction);
        }
        match facts.item_count {
            Some(count) if count >= LARGE_DESTRUCTIVE_ITEM_THRESHOLD => {
                risks.push(PreflightRisk::LargeItemCount);
            }
            None => risks.push(PreflightRisk::UnknownFacts),
            Some(_) => {}
        }
        match facts.byte_count {
            Some(bytes) if bytes >= LARGE_DESTRUCTIVE_BYTE_THRESHOLD => {
                risks.push(PreflightRisk::LargeByteCount);
            }
            None => risks.push(PreflightRisk::UnknownFacts),
            Some(_) => {}
        }
        match facts.max_depth {
            Some(depth) if depth >= LARGE_DESTRUCTIVE_DEPTH_THRESHOLD => {
                risks.push(PreflightRisk::DeepTree);
            }
            None => risks.push(PreflightRisk::UnknownFacts),
            Some(_) => {}
        }
        if facts.touches_filesystem_root {
            risks.push(PreflightRisk::FilesystemRoot);
        }
        if facts.touches_mount_root {
            risks.push(PreflightRisk::MountRoot);
        }
        if facts.touches_home {
            risks.push(PreflightRisk::HomeDirectory);
        }
        match facts.scan_state {
            PreflightScanState::Complete => {}
            PreflightScanState::Incomplete => risks.push(PreflightRisk::IncompleteScan),
            PreflightScanState::EntryLimitExceeded
            | PreflightScanState::DirectoryLimitExceeded
            | PreflightScanState::DepthLimitExceeded => {
                risks.push(PreflightRisk::ScanLimitExceeded);
            }
            PreflightScanState::ArithmeticOverflow => {
                risks.push(PreflightRisk::ArithmeticOverflow);
            }
        }
        risks.sort_by_key(|risk| risk_order(*risk));
        risks.dedup();
        if risks.is_empty() {
            Self::Proceed
        } else {
            Self::Confirm(risks)
        }
    }

    pub const fn requires_confirmation(&self) -> bool {
        matches!(self, Self::Confirm(_))
    }

    pub fn risks(&self) -> &[PreflightRisk] {
        match self {
            Self::Proceed => &[],
            Self::Confirm(risks) => risks,
        }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DestructiveScope {
    action: DestructiveAction,
    targets: Vec<PathBuf>,
    destination: Option<PathBuf>,
}

impl DestructiveScope {
    pub fn new(
        action: DestructiveAction,
        targets: Vec<PathBuf>,
        destination: Option<PathBuf>,
    ) -> Result<Self, DestructiveScopeError> {
        if targets.is_empty() {
            return Err(DestructiveScopeError::Empty);
        }
        if targets.len() > GUARDRAIL_TARGET_CAPACITY {
            return Err(DestructiveScopeError::CapacityExceeded {
                count: targets.len(),
                capacity: GUARDRAIL_TARGET_CAPACITY,
            });
        }

        let mut seen = HashSet::with_capacity(targets.len());
        for target in &targets {
            validate_guardrail_path(target).map_err(DestructiveScopeError::Path)?;
            if !seen.insert(target.clone()) {
                return Err(DestructiveScopeError::Duplicate(target.clone()));
            }
        }
        if let Some(destination) = destination.as_deref() {
            validate_guardrail_path(destination).map_err(DestructiveScopeError::Path)?;
        }

        Ok(Self {
            action,
            targets,
            destination,
        })
    }

    pub const fn action(&self) -> DestructiveAction {
        self.action
    }

    pub fn targets(&self) -> &[PathBuf] {
        &self.targets
    }

    pub fn destination(&self) -> Option<&Path> {
        self.destination.as_deref()
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DestructiveScopeError {
    #[error("destructive guardrail scope requires at least one target")]
    Empty,
    #[error("destructive target count {count} exceeds capacity {capacity}")]
    CapacityExceeded { count: usize, capacity: usize },
    #[error("destructive target occurs more than once: {}", .0.display())]
    Duplicate(PathBuf),
    #[error(transparent)]
    Path(#[from] ProtectedRootsError),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GuardrailPermit {
    id: u64,
}

#[derive(Debug, Default)]
pub struct GuardrailPermitIssuer {
    next_id: u64,
    active: HashMap<u64, PermitBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PermitBinding {
    policy_generation: u64,
    scope: DestructiveScope,
}

impl GuardrailPermitIssuer {
    pub fn issue(
        &mut self,
        policy_generation: u64,
        scope: DestructiveScope,
    ) -> Result<GuardrailPermit, GuardrailPermitError> {
        if self.active.len() == GUARDRAIL_ACTIVE_PERMIT_CAPACITY {
            return Err(GuardrailPermitError::CapacityExceeded);
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(GuardrailPermitError::IdExhausted)?;
        let permit = GuardrailPermit { id: self.next_id };
        self.active.insert(
            permit.id,
            PermitBinding {
                policy_generation,
                scope,
            },
        );
        Ok(permit)
    }

    /// Consume and validate a permit exactly once.
    ///
    /// Any validation attempt consumes the permit, including a stale or
    /// substituted attempt, so a rejected confirmation cannot be replayed.
    pub fn consume(
        &mut self,
        permit: GuardrailPermit,
        policy_generation: u64,
        scope: &DestructiveScope,
    ) -> Result<(), GuardrailPermitError> {
        let Some(binding) = self.active.remove(&permit.id) else {
            return Err(GuardrailPermitError::ReplayOrUnknown);
        };
        if binding.policy_generation != policy_generation {
            return Err(GuardrailPermitError::StalePolicy {
                expected: binding.policy_generation,
                actual: policy_generation,
            });
        }
        if binding.scope.action != scope.action {
            return Err(GuardrailPermitError::ActionSubstitution);
        }
        if binding.scope.targets != scope.targets {
            return Err(GuardrailPermitError::TargetSubstitution);
        }
        if binding.scope.destination != scope.destination {
            return Err(GuardrailPermitError::DestinationSubstitution);
        }
        Ok(())
    }

    pub fn revoke_all(&mut self) {
        self.active.clear();
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GuardrailPermitError {
    #[error("too many guardrail permits are awaiting consumption")]
    CapacityExceeded,
    #[error("guardrail permit identifier space is exhausted")]
    IdExhausted,
    #[error("guardrail permit was already consumed, revoked, or was never issued")]
    ReplayOrUnknown,
    #[error("guardrail permit policy is stale (expected generation {expected}, actual {actual})")]
    StalePolicy { expected: u64, actual: u64 },
    #[error("guardrail permit action was substituted")]
    ActionSubstitution,
    #[error("guardrail permit targets were substituted or reordered")]
    TargetSubstitution,
    #[error("guardrail permit destination was substituted")]
    DestinationSubstitution,
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };

    use tempfile::tempdir;

    use super::*;

    fn facts() -> DestructiveFacts {
        DestructiveFacts::new(
            Some(1),
            Some(1),
            Some(0),
            PreflightScanState::Complete,
            Vec::new(),
            false,
            false,
            false,
        )
    }

    #[test]
    fn phase_18x_protected_matches_raw_paths_and_component_boundaries() {
        let raw = PathBuf::from(OsString::from_vec(b"/safe/raw-\xff".to_vec()));
        let policy = ProtectedRoots::new(vec![PathBuf::from("/safe/two"), raw.clone()])
            .expect("protected roots");

        assert_eq!(
            policy.intersections(&raw).expect("exact")[0].relation(),
            ProtectedRelation::Exact
        );
        assert_eq!(
            policy
                .intersections(&raw.join("child"))
                .expect("descendant")[0]
                .relation(),
            ProtectedRelation::Descendant
        );
        assert_eq!(
            policy
                .intersections(Path::new("/safe"))
                .expect("ancestor")
                .len(),
            2
        );
        assert!(
            !policy
                .intersects(Path::new("/safe/twenty"))
                .expect("component boundary")
        );
    }

    #[test]
    fn phase_18x_protected_matching_is_explicitly_lexical_across_symlink_aliases() {
        let fixture = tempdir().expect("fixture");
        let protected = fixture.path().join("protected");
        std::fs::create_dir(&protected).expect("protected directory");
        let alias = fixture.path().join("alias");
        symlink(&protected, &alias).expect("alias");
        let policy = ProtectedRoots::new(vec![protected]).expect("policy");

        assert!(
            !policy
                .intersects(&alias)
                .expect("lexical alias remains distinct")
        );
    }

    #[test]
    fn phase_18x_protected_rejects_relative_duplicate_and_over_capacity_roots() {
        assert!(matches!(
            ProtectedRoots::new(vec![PathBuf::from("relative")]),
            Err(ProtectedRootsError::Relative(_))
        ));
        assert!(matches!(
            ProtectedRoots::new(vec![PathBuf::from("/one/./two")]),
            Err(ProtectedRootsError::Unnormalized(_))
        ));
        assert!(matches!(
            ProtectedRoots::new(vec![PathBuf::from("/one"), PathBuf::from("/one")]),
            Err(ProtectedRootsError::Duplicate(_))
        ));
        assert!(matches!(
            ProtectedRoots::new(
                (0..=PROTECTED_ROOT_CAPACITY)
                    .map(|index| PathBuf::from(format!("/root-{index}")))
                    .collect()
            ),
            Err(ProtectedRootsError::CapacityExceeded { .. })
        ));
    }

    #[test]
    fn phase_18x_preflight_is_deterministic_and_never_calls_unknown_safe() {
        assert_eq!(
            PreflightDecision::evaluate(DestructiveAction::Trash, &facts()),
            PreflightDecision::Proceed
        );
        assert_eq!(
            PreflightDecision::evaluate(DestructiveAction::PermanentDelete, &facts()).risks(),
            &[PreflightRisk::IrreversibleAction]
        );

        let unknown = DestructiveFacts::new(
            None,
            None,
            None,
            PreflightScanState::ArithmeticOverflow,
            Vec::new(),
            false,
            false,
            false,
        );
        assert_eq!(
            PreflightDecision::evaluate(DestructiveAction::Trash, &unknown).risks(),
            &[
                PreflightRisk::ArithmeticOverflow,
                PreflightRisk::UnknownFacts
            ]
        );

        let scope = DestructiveScope::new(
            DestructiveAction::Overwrite,
            vec![PathBuf::from("/target")],
            Some(PathBuf::from("/destination")),
        )
        .expect("scope");
        let mut issuer = GuardrailPermitIssuer::default();
        let permit = issuer.issue(2, scope.clone()).expect("override permit");
        assert_eq!(issuer.consume(permit, 2, &scope), Ok(()));
        assert_eq!(
            issuer.consume(permit, 2, &scope),
            Err(GuardrailPermitError::ReplayOrUnknown)
        );
    }

    #[test]
    fn phase_18x_preflight_threshold_boundaries_require_confirmation() {
        let large = DestructiveFacts::new(
            Some(LARGE_DESTRUCTIVE_ITEM_THRESHOLD),
            Some(LARGE_DESTRUCTIVE_BYTE_THRESHOLD),
            Some(LARGE_DESTRUCTIVE_DEPTH_THRESHOLD),
            PreflightScanState::Complete,
            Vec::new(),
            false,
            false,
            false,
        );
        assert_eq!(
            PreflightDecision::evaluate(DestructiveAction::Move, &large).risks(),
            &[
                PreflightRisk::LargeItemCount,
                PreflightRisk::LargeByteCount,
                PreflightRisk::DeepTree,
            ]
        );
    }

    #[test]
    fn phase_18x_permit_is_single_use_and_bound_to_policy_action_scope() {
        let scope =
            DestructiveScope::new(DestructiveAction::Trash, vec![PathBuf::from("/one")], None)
                .expect("scope");
        let mut issuer = GuardrailPermitIssuer::default();
        let permit = issuer.issue(7, scope.clone()).expect("permit");
        assert_eq!(issuer.consume(permit, 7, &scope), Ok(()));
        assert_eq!(
            issuer.consume(permit, 7, &scope),
            Err(GuardrailPermitError::ReplayOrUnknown)
        );

        let permit = issuer.issue(7, scope.clone()).expect("stale permit");
        assert!(matches!(
            issuer.consume(permit, 8, &scope),
            Err(GuardrailPermitError::StalePolicy { .. })
        ));

        let permit = issuer.issue(8, scope.clone()).expect("substitution permit");
        let substituted =
            DestructiveScope::new(DestructiveAction::Trash, vec![PathBuf::from("/two")], None)
                .expect("substituted scope");
        assert_eq!(
            issuer.consume(permit, 8, &substituted),
            Err(GuardrailPermitError::TargetSubstitution)
        );
        assert_eq!(
            issuer.consume(permit, 8, &scope),
            Err(GuardrailPermitError::ReplayOrUnknown)
        );

        let permit = issuer.issue(9, scope.clone()).expect("action permit");
        let action_substitution = DestructiveScope::new(
            DestructiveAction::PermanentDelete,
            scope.targets().to_vec(),
            None,
        )
        .expect("action substitution");
        assert_eq!(
            issuer.consume(permit, 9, &action_substitution),
            Err(GuardrailPermitError::ActionSubstitution)
        );

        let destination_scope = DestructiveScope::new(
            DestructiveAction::Move,
            vec![PathBuf::from("/one")],
            Some(PathBuf::from("/destination-one")),
        )
        .expect("destination scope");
        let permit = issuer
            .issue(10, destination_scope.clone())
            .expect("destination permit");
        let destination_substitution = DestructiveScope::new(
            DestructiveAction::Move,
            vec![PathBuf::from("/one")],
            Some(PathBuf::from("/destination-two")),
        )
        .expect("destination substitution");
        assert_eq!(
            issuer.consume(permit, 10, &destination_substitution),
            Err(GuardrailPermitError::DestinationSubstitution)
        );

        let ordered_scope = DestructiveScope::new(
            DestructiveAction::Trash,
            vec![PathBuf::from("/one"), PathBuf::from("/two")],
            None,
        )
        .expect("ordered scope");
        let permit = issuer.issue(11, ordered_scope).expect("ordered permit");
        let reordered_scope = DestructiveScope::new(
            DestructiveAction::Trash,
            vec![PathBuf::from("/two"), PathBuf::from("/one")],
            None,
        )
        .expect("reordered scope");
        assert_eq!(
            issuer.consume(permit, 11, &reordered_scope),
            Err(GuardrailPermitError::TargetSubstitution)
        );
    }

    #[test]
    fn phase_18x_permit_capacity_is_bounded_and_revocation_prevents_use() {
        let scope =
            DestructiveScope::new(DestructiveAction::Trash, vec![PathBuf::from("/one")], None)
                .expect("scope");
        let mut issuer = GuardrailPermitIssuer::default();
        for _ in 0..GUARDRAIL_ACTIVE_PERMIT_CAPACITY {
            issuer.issue(1, scope.clone()).expect("bounded permit");
        }
        assert_eq!(
            issuer.issue(1, scope.clone()),
            Err(GuardrailPermitError::CapacityExceeded)
        );
        issuer.revoke_all();
        let permit = issuer.issue(1, scope.clone()).expect("permit after revoke");
        issuer.revoke_all();
        assert_eq!(
            issuer.consume(permit, 1, &scope),
            Err(GuardrailPermitError::ReplayOrUnknown)
        );
    }
}
