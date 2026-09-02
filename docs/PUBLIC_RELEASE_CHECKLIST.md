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
- [ ] Confirm the Git author email embedded in existing commits is intentionally
  public. The owner's GitHub profile does not currently publish an email. If it
  is private, rewrite history before visibility changes and re-clone/re-audit.
- [ ] Configure a public or GitHub noreply author identity before creating new
  public commits.
- [ ] Decide whether historical machine-specific home/agent paths are acceptable.
  The working tree must contain no personal absolute path; history still requires
  explicit acceptance or a rewrite before publication.
- [ ] Confirm no secrets, credentials, private keys, personal files, sensitive
  paths, machine logs, or private fixtures exist anywhere in Git history.
- [ ] Rotate/revoke any credential that ever entered history before rewriting;
  deleting it from the current tree is not sufficient.

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
