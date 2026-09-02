# Floe Public Repository and First Prerelease Checklist

This checklist separates repository-verified evidence from GitHub settings and
human decisions. Floe remains source-available proprietary software. Do not make
the repository public while a blocker below is unresolved.

The recommended first prerelease tag is `v0.1.0-alpha.1`. This document does not
authorize or create a tag, release, or visibility change.

## Repository and legal review

- [ ] Confirm [LICENSE](../LICENSE), Cargo `LicenseRef-proprietary` metadata,
  [CLA](../CLA.md), and public wording reflect the copyright holder's intended
  proprietary/source-available model.
- [ ] Obtain human or legal review of the CLA and binary-distribution obligations;
  automated SPDX checks are an inventory policy, not legal advice.
- [ ] Review all direct and transitive dependency licenses and bundled assets.
  Confirm required notices accompany every distributed package.
- [x] Rewrite every reachable local Git author email and committer email to the
  reviewed GitHub noreply address, and configure that identity for new commits.
- [x] Rewrite Git history so account-specific home, media, and agent paths use neutral
  placeholders. Compare all rewritten local refs against a verified pre-rewrite
  bundle, then remove rewrite backup refs and prune unreachable originals.
- [x] Complete tracked-tree and reachable-history audits. No high-confidence
  secret, credential, private-key, personal-file, or machine-log marker was
  found. This is review evidence, not proof that undiscovered sensitive data
  never existed.
- [ ] Keep the private pre-rewrite recovery bundle access-restricted and offline
  until remote migration and a fresh-clone audit succeed, then deliberately
  destroy it if recovery is no longer needed. Deletion is not secure erase.
- [ ] Before public visibility, coordinate replacement of every GitHub branch
  that still exposes pre-rewrite history. A normal push is insufficient; use a
  deliberate force-with-lease migration or delete obsolete remote branches,
  then re-clone and repeat the complete metadata/blob audit.
- [ ] If any credential is later found in current or prior history, rotate or
  revoke it; rewriting history alone does not invalidate a disclosed secret.

## Automated repository gates

- [x] Run `cargo fmt --all -- --check`.
- [x] Run `cargo check --workspace`.
- [x] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] Run `cargo test --workspace`.
- [x] Run strict documentation and rendered-document validation.
- [x] Run packaging layout, settings migration, deterministic source, and
  release-candidate policy tests.
- [x] Run dependency-license policy and current advisory review.
- [x] Run the E2E harness contract/preflight; report unavailable native graphical
  layers as not run, never as passed.
- [x] Run `git diff --check` and review the complete branch diff.
- [x] Run the local reachable-history identity/path gate and the full
  bundle-to-rewrite equivalence audit.
- [x] Build the verified native release package and smoke-test the staged package
  in a disposable environment.

## GitHub repository settings

- [ ] Commit the reviewed changes on the dedicated release-readiness branch,
  push that branch, and merge it through a pull request rather than pushing
  directly to `main`.
- [ ] Enable GitHub Private Vulnerability Reporting and verify the **Report a
  vulnerability** path while signed out or from a non-maintainer account.
- [ ] Keep GitHub Issues enabled.
- [ ] Enable GitHub Discussions only if maintainers want a separate support and
  design-discussion channel.
- [ ] Configure a `main` branch ruleset or branch protection.
- [ ] Require the **Rust quality gates** CI check before merging.
- [ ] Disable force pushes and branch deletion for `main` if appropriate to the
  maintainer workflow.
- [ ] Confirm `CODEOWNERS` resolves to the intended maintainer and optionally
  require code-owner review.
- [ ] Review the repository description, website, and topics. Do not label Floe
  open source.
- [ ] Confirm the logo and all README links/assets render correctly on GitHub.

## Prerelease publication

- [ ] Confirm CI passes on the final `main` commit.
- [ ] Confirm the version, changelog, AppStream metadata, Arch package metadata,
  deterministic source checksum, and documentation agree.
- [ ] Build and smoke-test the exact artifact intended for distribution.
- [ ] Create `v0.1.0-alpha.1` only after the reviewed commit is on `main`.
- [ ] Publish the first GitHub prerelease with the proprietary license and known
  limitations clearly visible.
- [ ] Change repository visibility to public only after every blocker and manual
  privacy/history check above is closed.
- [ ] Recheck public pages, issue forms, CI, Private Vulnerability Reporting,
  source archive contents, and downloadable artifacts after publication.
