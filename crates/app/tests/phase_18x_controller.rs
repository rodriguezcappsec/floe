#[path = "../src/guardrail_controller.rs"]
pub mod guardrail_controller;
#[path = "../src/guardrail_preflight.rs"]
pub mod guardrail_preflight;
#[path = "../src/guardrail_store.rs"]
pub mod guardrail_store;
#[path = "../src/guardrail_ui_model.rs"]
pub mod guardrail_ui_model;

use std::{
    ffi::OsString,
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    time::{Duration, Instant},
};

use floe_core::{
    DestructiveAction, DestructiveScope, GuardrailPermitError, PreflightRisk, ProtectedRoots,
};
use tempfile::tempdir;

use guardrail_controller::{
    GuardrailConfirmation, GuardrailController, GuardrailControllerError, GuardrailPoll,
    GuardrailResolution, GuardrailReview, GuardrailReviewRequest, GuardrailStoreHealth,
};
use guardrail_preflight::PreflightEnvironment;
use guardrail_store::GuardrailStore;
use guardrail_ui_model::{
    GUARDRAIL_LIMITATION_TEXT, GuardrailActionStates, GuardrailDialogModel, guardrail_action_states,
};

fn wait_for_terminal(controller: &mut GuardrailController, generation: u64) -> GuardrailPoll {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match controller.poll(generation).expect("poll") {
            GuardrailPoll::Pending => {
                assert!(Instant::now() < deadline, "guardrail controller timed out");
                std::thread::yield_now();
            }
            terminal => return terminal,
        }
    }
}

fn begin(controller: &mut GuardrailController, scopes: Vec<DestructiveScope>) -> GuardrailPoll {
    let request = GuardrailReviewRequest::new(scopes, PreflightEnvironment::default())
        .expect("review request");
    let generation = controller
        .begin_review(request)
        .expect("begin")
        .generation();
    wait_for_terminal(controller, generation)
}

fn review(terminal: GuardrailPoll) -> GuardrailReview {
    match terminal {
        GuardrailPoll::ReviewRequired(review) => review,
        other => panic!("expected review, received {other:?}"),
    }
}

#[test]
fn phase_18x_controller_safe_scope_auto_allows_and_permit_is_single_use() {
    let fixture = tempdir().expect("fixture");
    let target = fixture.path().join("small");
    fs::write(&target, b"small").expect("target");
    let mut controller =
        GuardrailController::load_at(fixture.path().join("missing/guardrails.bin"))
            .expect("controller");
    assert_eq!(controller.store_health(), GuardrailStoreHealth::Missing);
    let scope = DestructiveScope::new(DestructiveAction::Trash, vec![target], None).expect("scope");
    let authorization = match begin(&mut controller, vec![scope.clone()]) {
        GuardrailPoll::Allowed(authorization) => authorization,
        other => panic!("safe scope was not auto-allowed: {other:?}"),
    };
    let item = authorization.items()[0].clone();
    controller
        .consume_authorization(item.clone(), &scope)
        .expect("first consume");
    assert_eq!(
        controller.consume_authorization(item, &scope),
        Err(GuardrailPermitError::ReplayOrUnknown)
    );
}

#[test]
fn phase_18x_controller_protected_large_and_permanent_scopes_require_confirmation() {
    let fixture = tempdir().expect("fixture");
    let protected = fixture.path().join("protected");
    fs::create_dir(&protected).expect("protected");
    let policy = ProtectedRoots::with_generation(7, vec![protected.clone()]).expect("policy");
    let store = fixture.path().join("private/guardrails.bin");
    GuardrailStore::persist(&store, &policy).expect("persist policy");
    let mut controller = GuardrailController::load_at(store).expect("controller");

    let protected_scope =
        DestructiveScope::new(DestructiveAction::Trash, vec![protected.clone()], None)
            .expect("protected scope");
    let protected_review = review(begin(&mut controller, vec![protected_scope]));
    assert_eq!(protected_review.risks(), &[PreflightRisk::ProtectedPath]);
    assert!(matches!(
        controller
            .resolve_review(protected_review.generation(), GuardrailConfirmation::Deny)
            .expect("deny"),
        GuardrailResolution::Denied
    ));

    let permanent_scope = DestructiveScope::new(
        DestructiveAction::PermanentDelete,
        vec![fixture.path().join("permanent")],
        None,
    )
    .expect("permanent scope");
    fs::write(permanent_scope.targets()[0].as_path(), b"delete").expect("permanent target");
    let permanent_review = review(begin(&mut controller, vec![permanent_scope]));
    assert_eq!(
        permanent_review.risks(),
        &[PreflightRisk::IrreversibleAction]
    );
    assert!(matches!(
        controller
            .resolve_review(
                permanent_review.generation(),
                GuardrailConfirmation::Confirm
            )
            .expect("confirm"),
        GuardrailResolution::Allowed(_)
    ));

    let large = fixture.path().join("large");
    fs::create_dir(&large).expect("large root");
    for index in 0..999 {
        fs::write(large.join(index.to_string()), []).expect("large member");
    }
    let large_scope =
        DestructiveScope::new(DestructiveAction::Move, vec![large], None).expect("large scope");
    let large_review = review(begin(&mut controller, vec![large_scope]));
    assert_eq!(large_review.risks(), &[PreflightRisk::LargeItemCount]);
}

#[test]
fn phase_18x_controller_aggregates_exact_mixed_scopes_in_stable_order() {
    let fixture = tempdir().expect("fixture");
    let protected = fixture.path().join("protected");
    let ordinary = fixture.path().join("ordinary");
    fs::write(&protected, b"protected").expect("protected target");
    fs::write(&ordinary, b"ordinary").expect("ordinary target");
    let policy = ProtectedRoots::with_generation(3, vec![protected.clone()]).expect("policy");
    let store = fixture.path().join("private/guardrails.bin");
    GuardrailStore::persist(&store, &policy).expect("persist");
    let mut controller = GuardrailController::load_at(store).expect("controller");
    let protected_scope =
        DestructiveScope::new(DestructiveAction::Trash, vec![protected], None).expect("scope");
    let permanent_scope =
        DestructiveScope::new(DestructiveAction::PermanentDelete, vec![ordinary], None)
            .expect("scope");
    let expected = vec![protected_scope.clone(), permanent_scope.clone()];
    let review = review(begin(&mut controller, expected.clone()));
    assert_eq!(
        review.risks(),
        &[
            PreflightRisk::ProtectedPath,
            PreflightRisk::IrreversibleAction
        ]
    );
    assert_eq!(review.scopes().cloned().collect::<Vec<_>>(), expected);
    let authorization = match controller
        .resolve_review(review.generation(), GuardrailConfirmation::Confirm)
        .expect("confirm")
    {
        GuardrailResolution::Allowed(authorization) => authorization,
        GuardrailResolution::Denied => panic!("confirmation was denied"),
    };
    assert_eq!(authorization.items().len(), 2);
    assert_eq!(authorization.items()[0].scope(), &protected_scope);
    assert_eq!(authorization.items()[1].scope(), &permanent_scope);
}

#[test]
fn phase_18x_controller_corrupt_store_blocks_until_explicit_acknowledged_reset() {
    let fixture = tempdir().expect("fixture");
    let private = fixture.path().join("private");
    fs::create_dir(&private).expect("private");
    fs::set_permissions(
        &private,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .expect("private mode");
    let store = private.join("guardrails.bin");
    fs::write(&store, b"corrupt").expect("corrupt");
    fs::set_permissions(&store, std::os::unix::fs::PermissionsExt::from_mode(0o600))
        .expect("file mode");
    let target = fixture.path().join("target");
    fs::write(&target, b"target").expect("target");
    let scope = DestructiveScope::new(DestructiveAction::Trash, vec![target], None).expect("scope");
    let request = GuardrailReviewRequest::new(vec![scope.clone()], PreflightEnvironment::default())
        .expect("request");
    let mut controller = GuardrailController::load_at(store).expect("controller");
    assert_eq!(controller.store_health(), GuardrailStoreHealth::Blocked);
    assert!(controller.store_error().is_some());
    assert!(matches!(
        controller.begin_review(request.clone()),
        Err(GuardrailControllerError::StoreBlocked)
    ));
    assert!(matches!(
        controller.acknowledge_and_reset_blocked_store(false),
        Err(GuardrailControllerError::StoreBlocked)
    ));
    controller
        .acknowledge_and_reset_blocked_store(true)
        .expect("acknowledged reset");
    assert_eq!(controller.store_health(), GuardrailStoreHealth::Ready);
    controller.begin_review(request).expect("unblocked begin");

    let model = GuardrailDialogModel::store_blocked();
    assert!(model.heading().contains("remain blocked"));
    assert!(model.confirm_label().is_none());
}

#[test]
fn phase_18x_controller_cancel_and_stale_generation_never_authorize() {
    let fixture = tempdir().expect("fixture");
    let target = fixture.path().join("target");
    fs::write(&target, b"target").expect("target");
    let store = fixture.path().join("missing/guardrails.bin");
    let mut controller = GuardrailController::load_at(store).expect("controller");
    let scope = DestructiveScope::new(DestructiveAction::Trash, vec![target], None).expect("scope");
    let request = GuardrailReviewRequest::new(vec![scope.clone()], PreflightEnvironment::default())
        .expect("request");
    let generation = controller
        .begin_review(request.clone())
        .expect("begin")
        .generation();
    controller.cancel(generation).expect("cancel");
    assert!(matches!(
        wait_for_terminal(&mut controller, generation),
        GuardrailPoll::Cancelled
    ));

    let authorization = match begin(&mut controller, vec![scope.clone()]) {
        GuardrailPoll::Allowed(authorization) => authorization,
        other => panic!("expected authorization: {other:?}"),
    };
    let mut revised = controller.policy().clone();
    revised
        .add(PathBuf::from("/new-protected-root"))
        .expect("new generation");
    controller
        .install_persisted_policy(revised)
        .expect("install policy");
    assert_eq!(
        controller.consume_authorization(authorization.items()[0].clone(), &scope),
        Err(GuardrailPermitError::ReplayOrUnknown)
    );
}

#[test]
fn phase_18x_ui_summary_preserves_exact_raw_scope_and_truthful_limitations() {
    let fixture = tempdir().expect("fixture");
    let raw_target = fixture
        .path()
        .join(OsString::from_vec(b"raw-target-\xff".to_vec()));
    fs::write(&raw_target, b"raw").expect("raw target");
    let raw_destination = fixture
        .path()
        .join(OsString::from_vec(b"raw-destination-\xfe".to_vec()));
    let policy = ProtectedRoots::new(vec![fixture.path().to_path_buf()]).expect("policy");
    let store = fixture.path().join("private/guardrails.bin");
    GuardrailStore::persist(&store, &policy).expect("persist");
    let mut controller = GuardrailController::load_at(store).expect("controller");
    let scope = DestructiveScope::new(
        DestructiveAction::Move,
        vec![raw_target.clone()],
        Some(raw_destination.clone()),
    )
    .expect("scope");
    let review = review(begin(&mut controller, vec![scope]));
    let model = GuardrailDialogModel::confirmation(&review);
    assert_eq!(model.exact_target_count(), 1);
    let displayed = &model.scopes()[0];
    assert_eq!(
        displayed.targets()[0].exact_path().as_os_str().as_bytes(),
        raw_target.as_os_str().as_bytes()
    );
    assert_eq!(
        displayed
            .destination()
            .expect("destination")
            .exact_path()
            .as_os_str()
            .as_bytes(),
        raw_destination.as_os_str().as_bytes()
    );
    assert!(displayed.targets()[0].visible_path().contains("\\xFF"));
    assert!(model.accessible_label().contains("1 exact target"));
    assert_eq!(model.limitation(), GUARDRAIL_LIMITATION_TEXT);
    let limitation = model.limitation().to_ascii_lowercase();
    assert!(limitation.contains("do not encrypt"));
    assert!(limitation.contains("provide access control"));
    assert!(!limitation.contains("encrypted folder"));
    assert!(!limitation.contains("prevents access"));
}

#[test]
fn phase_18x_protect_unprotect_state_uses_exact_raw_path_membership() {
    let fixture = tempdir().expect("fixture");
    let raw = fixture
        .path()
        .join(OsString::from_vec(b"protected-\xff".to_vec()));
    let same_visible_but_distinct = fixture
        .path()
        .join(OsString::from_vec(b"protected-\xfe".to_vec()));
    let policy = ProtectedRoots::new(vec![raw.clone()]).expect("policy");

    assert_eq!(
        guardrail_action_states(Some(&raw), &policy, false, false),
        GuardrailActionStates {
            protect: false,
            unprotect: true,
        }
    );
    assert_eq!(
        guardrail_action_states(Some(&same_visible_but_distinct), &policy, false, false),
        GuardrailActionStates {
            protect: true,
            unprotect: false,
        }
    );
    for states in [
        guardrail_action_states(Some(&raw), &policy, true, false),
        guardrail_action_states(Some(&raw), &policy, false, true),
        guardrail_action_states(None, &policy, false, false),
    ] {
        assert_eq!(
            states,
            GuardrailActionStates {
                protect: false,
                unprotect: false,
            }
        );
    }
}
