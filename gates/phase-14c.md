# Gates: Floe Phase 14C — Safe Administrator Operations

Scope: Explicit bounded administrator mutations through a separate GIO/GVfs
boundary; no whole-process elevation or local-job URI stripping.

- [x] A1: Typed policy preserves raw identities and rejects relative, mixed, or
  unsafe names, roots, modes, overwrite, and unsupported recursion before I/O.
  CHECK: `cargo test -p floe-app phase_14c_policy -- --nocapture`
  EVIDENCE: Focused policy tests pass for raw identity, strict child names,
  no-follow fingerprints, mode bounds, authority separation, and unsupported
  recursive copy rejection.
- [x] A2: Capacity-one service emits structured progress, cancellation, and
  terminal outcomes with fresh identity and redacted errors.
  CHECK: `cargo test -p floe-app phase_14c_service -- --nocapture`
  EVIDENCE: Focused service tests pass; requests use typed GFiles,
  `NOFOLLOW_SYMLINKS`, no overwrite flag, distinct cancellation-requested
  state, and explicit possible-partial-destination failure evidence.
- [x] A3: Administrator view exposes accessible New Folder, Rename, Copy/Move,
  Trash, Delete Permanently, and Permissions controls with explicit scope and
  risk confirmation.
  CHECK: `cargo test -p floe-app phase_14c_ui -- --nocapture`
  EVIDENCE: Deterministic and ignored real-GTK Phase 14C UI/accessibility tests
  pass. Every mutation has explicit accessible confirmation and the dialog
  cannot close or accept Return while work is active.
- [x] A4: Real GTK/native Wayland keeps process UID stable, administrator state
  visible, liveness responsive, and Quit clean.
  EVIDENCE: Isolated KDE Wayland answered D-Bus `Peer.Ping`, exported
  `org.gtk.Actions`, accepted Quit, and exited 0; marker recorded PID 449992,
  UID 1000. A disposable root-owned mutation fixture was unavailable, so an
  actual privileged mutation is not claimed.
- [x] A5: Formatting, check, strict Clippy, workspace tests, docs/release, diff
  hygiene, and exactly one next phase pass.
  EVIDENCE: `cargo fmt --all -- --check`, workspace check, strict all-target and
  all-feature Clippy, workspace tests, strict docs/rendering, package layout and
  migrations, deterministic release source/candidate, E2E contract discovery,
  and diff hygiene all exit 0. Release-source SHA-256 is
  `cae002ea5f0259ee0a35112eeaf19cf6a81cff63b049dff1f0b5524f86fd9235`.
