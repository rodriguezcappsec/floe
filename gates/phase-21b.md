# Gates: Floe Phase 21B — Packaging and migrations

Scope: verified native Arch/source packaging, stable release identity, and safe
versioned settings/cache migration without changing user defaults or inventing
an Encrypted Vault migration.

- [x] P1: Desktop/AppStream/icon/resource/application/binary identity validates.
  CHECK: `desktop-file-validate data/io.github.rodriguezcappsec.Floe.desktop && appstreamcli validate --no-net data/io.github.rodriguezcappsec.Floe.metainfo.xml && cargo test -p floe-app phase_21b_release_metadata -- --nocapture`
  EXPECT: `/test result: ok/`
  EVIDENCE: Both native validators pass and two release-metadata tests pass; the
  stable identity is `io.github.rodriguezcappsec.Floe`, command `floe`, and only
  `inode/directory` is advertised.

- [x] P2: Frozen optimized binary and exact source/package layout validate
  without user-XDG or default-MIME mutation.
  CHECK: `cargo build --frozen --release -p floe-app --bin floe && sh packaging/tests/test-package-layout.sh`
  EXPECT: `/phase-21b-package-layout-ok/`
  EVIDENCE: Frozen x86-64 PIE builds with no missing dynamic libraries; staged
  install/uninstall owns exactly the manifest paths and preserves XDG sentinels.

- [x] P3: Private preference migration and rollback contracts cover supported
  and hostile inputs below disposable XDG roots.
  CHECK: `cargo test -p floe-app phase_21b_migration -- --nocapture && sh packaging/tests/test-migrations.sh`
  EXPECT: `/phase-21b-migrations-ok/`
  EVIDENCE: Three focused tests plus the disposable migration suite pass clean,
  legacy, corrupt, oversized, symlink, future-version, interruption-residue,
  cache-rebuild, backup, and rollback cases; no vault format exists or migrates.

- [x] P4: Deterministic source and real Arch package build are reproducible.
  CHECK: `sh packaging/release-source.sh /tmp/floe-phase21b-source.tar.gz && (cd /tmp/floe-phase21b-final.iGNAoI && makepkg --cleanbuild --nodeps --nocheck)`
  EXPECT: `/Finished making: floe 0.1.0-1/`
  EVIDENCE: Two archives reproduced SHA-256
  `ae74aca57f9ed6ef9fdbcb21858d6e5e578679263757256f3cd08c7c06f264c0`;
  PKGBUILD validation, frozen clean build, fakeroot install, package checks, and
  `floe-0.1.0-1-x86_64.pkg.tar.zst` creation pass. Host C-LTO is disabled for
  this package because it broke static liblzma; Rust thin-LTO remains enabled.

- [x] P5: Staged installed native Wayland launch accepts one local directory,
  answers Ping, exports Quit, and exits cleanly.
  EVIDENCE: Isolated staged binary owned the stable D-Bus name, answered Peer
  Ping, exported Quit, persisted the exact fixture path, and exited 0. The only
  critical was the known host refusal of the AT-SPI accessibility-bus socket;
  no semantic E2E claim is made.

- [x] P6: Persistent docs agree on verified packaging/migration/legal limits
  and exactly Phase 21C is `NEXT`.
  CHECK: `test "$(rg -c '\| NEXT \|' docs/ROADMAP.md)" -eq 1 && rg -n '21B.*COMPLETE|21C.*NEXT' docs/ROADMAP.md`
  EXPECT: `/21C.*NEXT/`
  EVIDENCE: README, installation, migrations, development, architecture,
  privacy/security, feature matrix, roadmap, AGENTS, PLAN, and gate ledgers
  record the verified Arch/source boundary and deferred Flatpak/publication.

- [x] P7: Full deterministic release gates pass.
  CHECK: `cargo fmt --all -- --check && cargo check --workspace && cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo test --workspace && cargo build --frozen --release -p floe-app --bin floe && git diff --check`
  EXPECT: `/Finished/`
  EVIDENCE: Format, workspace check, strict all-target/all-feature Clippy,
  workspace tests, frozen release, metadata/layout/migration validators, E2E
  preflight, and diff hygiene pass. Tests: 554 app passed with 14 intentional
  graphical ignores, 21 controller, 162 core, and six duplicate workflows;
  native E2E skipped exact missing Dogtail/pyatspi dependency.
