# Gates: Floe Phase 5F conflict interaction

Scope: Present Phase 5E pending conflicts through a focused, non-blocking GTK interaction. Users can keep the existing destination or retry copy/move/rename with one validated sibling name. Dismissal must leave the conflict recoverable. Overwrite, apply-to-all, permanent deletion, and filesystem work in GTK callbacks remain unavailable.

- [x] G1: Branch is `phase-5f-conflict-interaction` and `main` remains at Phase 5E.
  CHECK: git branch --show-current && git rev-parse main
  EXPECT: /phase-5f-conflict-interaction[\s\S]*00a2a59/
  EVIDENCE: branch is `phase-5f-conflict-interaction`; local `main` remains Phase 5E commit `00a2a59`.

- [x] G2: A destination-conflict event presents one non-blocking, transient GTK decision surface with clear source/destination context and initial keyboard focus.
  CHECK: rg -n "ConflictDialog|present_conflict|set_transient_for|present|grab_focus" crates/app/src
  EXPECT: /present_conflict/
  EVIDENCE: conflict outcomes call `present_conflict`; one `AdwDialog` shows incoming/existing paths, presents on the application window, and focuses the filename entry.

- [x] G3: The surface exposes exactly keep-existing and validated retry-with-name decisions; overwrite and apply-to-all are absent.
  CHECK: rg -n "Keep Existing|Retry with New Name|Overwrite|Apply to All|ConflictDecision" crates/app/src
  EXPECT: /Keep Existing/
  EVIDENCE: `CONFLICT_DECISION_LABELS` contains exactly Keep Existing and Retry with New Name; a focused test rejects overwrite/apply-to-all labels.

- [x] G4: Name validation is immediate and accessible, one raw filename is submitted through `ApplicationState::resolve_conflict`, and GTK performs no filesystem mutation.
  CHECK: rg -n "validate_rename_name|resolve_conflict|set_sensitive|error" crates/app/src/operations.rs crates/app/src/ui.rs
  EXPECT: /resolve_conflict/
  EVIDENCE: entry changes run shared single-component validation, update button sensitivity and an associated inline error, then submit `ConflictDecision` through application state only.

- [x] G5: Dismissal leaves the conflict pending and the Operations Island exposes an accessible Resolve Conflict action that can reopen it.
  CHECK: rg -n "Resolve Conflict|pending_conflict|conflict_job|dismiss" crates/app/src/operations.rs crates/app/src/ui.rs
  EXPECT: /Resolve Conflict/
  EVIDENCE: `dismiss_dialog` clears only active-dialog identity; the ordered pending queue remains and drives the labelled Resolve Conflict island action.

- [x] G6: Keep-existing resolves without a new job; retry-name transitions to the fresh attempt and preserves normal progress/error handling.
  CHECK: rg -n "KeptExisting|Retried|ConflictResolution|track_active|show_running" crates/app/src/operations.rs
  EXPECT: /ConflictResolution/
  EVIDENCE: `KeptExisting` closes with resolved feedback and no submission; `Retried` tracks the returned fresh job and calls the normal running-state presenter.

- [x] G7: Focused Phase 5F tests cover action availability, validation, dismiss/reopen, keep-existing, retry submission, and no overwrite/apply-to-all option.
  CHECK: cargo test -p floe-app phase_5f -- --nocapture
  EXPECT: /test result: ok/
  EVIDENCE: four focused Phase 5F tests pass for conflict priority, dismiss/reopen/single-dialog state, exact validation, keep-existing/fresh retry submission, retry fallback, and exact non-overwrite decision labels.

- [x] G8: Documentation and project status describe Phase 5F behavior, limitations, verification, and the next coherent branch.
  CHECK: rg -n "Phase 5F|Resolve Conflict|phase-6a" README.md DESIGN.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md docs/ROADMAP.md AGENTS.md
  EXPECT: /phase-6a/
  EVIDENCE: README, DESIGN, architecture, development, roadmap, and AGENTS status document Phase 5F, its limits, and `phase-6a-list-view-polish` next.

- [x] G9: Formatting, workspace compilation, strict Clippy, all tests, diff hygiene, gate check, and native Wayland smoke pass.
  CHECK: cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && git diff --check
  EXPECT: /test result: ok/
  EVIDENCE: fmt, workspace check, strict Clippy, all 85 tests (28 core, 57 app), and diff hygiene pass; native Wayland launch logged startup and remained healthy until timeout with only known host warnings.
