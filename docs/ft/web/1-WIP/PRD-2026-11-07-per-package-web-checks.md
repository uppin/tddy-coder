# Per-package web PR checks - PRD

**Date**: 2026-11-07
**PRD Type**: Technical Improvement
**Status**: 🚧 In Progress
**Stack**: `#web-split` 1/16 — branch `feature/web-split/web-ci`

## Affected Features

- **Primary**: Web development workflow — [local-web-dev.md](../local-web-dev.md) (how a contributor runs
  one web package's tests), and the CI guide [docs/dev/guides/ci.md](../../../dev/guides/ci.md) (what each
  PR check covers).
- **Related**: every JS workspace package listed in the root `package.json` `workspaces`
  (`tddy-web`, `tddy-livekit-web`, `tddy-rpc-web`, `tddy-tauri-web`, `tddy-desktop`,
  `tddy-rust-typescript-tests`, `tddy-connectrpc-testkit`) — each gains its own PR check.

## Summary

Every JS workspace package gets **its own GitHub workflow and its own PR check**. A pull request runs
a package's tests **only when the PR touches that package, or a workspace package it depends on**. The
single `Web tests` job in `ci.yml`, which runs every web suite on every PR, is removed.

## Background

`ci.yml`'s `web` job ("Web tests") builds the whole web bundle and runs every unit, transport and
Cypress suite for every PR, Rust-only PRs included. It needs `rust-build` first (for the LiveKit
echo-server fixture), so a one-line web fix waits on a full Rust build and a ~50-minute Cypress run.
The `#web-split` stack is about to cut `tddy-web` into a dozen packages; one monolithic job would hide
which package broke and keep paying for all of them on every change. The check per package must exist
**before** the packages do, so each new package can add its workflow as it is created.

## Proposed Changes

### What's Changing

- **One workflow per package**: `.github/workflows/web-<package>.yml`, each calling one shared
  reusable workflow with its package name. The check a PR shows is named after the package
  (e.g. `tddy-rpc-web tests`).
- **Affected-package detection**: a script resolves a PR's changed files to the set of workspace
  packages affected — a package is affected when a file under it changed, when a workspace package it
  depends on (`workspace:*` in `dependencies`/`devDependencies`, transitively) is affected, or when a
  **global input** changed (root `package.json`, `bun.lock`, the shared workflow, the detection
  script itself, `flake.nix`/`flake.lock`). Pushes to `master` and manual dispatches run everything.
- **An unaffected package's check passes as skipped** — the workflow always starts, decides, and
  skips its test job, so the check is present (and green) on every PR. A required check therefore
  never hangs waiting for a workflow that did not trigger.
- **Draft PRs follow the existing build gate** — drafts skip unless `#forcebuild`, exactly as today.
- **A rule test** fails when a workspace package has no workflow, or a workflow names a package that
  is not in the workspace — so a package added later cannot ship without its check.
- **The CI scripts' own tests run in CI** — the detection script's tests, plus the three script test
  files the backlog records as run only locally.
- `ci.yml`'s `web` job is deleted. Rust jobs are unchanged.

### What's Staying the Same

- Every test that runs today still runs on a PR that touches its package, with the same commands and
  the same JUnit reporting.
- The Rust checks (`Rust tests`, `Rust lint`, `Rust build`) and `vm-tests.yml` are untouched.
- `#forcebuild`, `#automerge` and the draft gate behave as today.

## Impact Analysis

### Technical Impact

- New: `scripts/web-affected.ts` (+ test), `.github/workflows/web-package.yml` (reusable),
  `web-<package>.yml` × 7. Removed: the `web` job in `ci.yml`.
- `tddy-livekit-web`'s Cypress specs need the `echo_server` example binary that `rust-build` uploads
  today. Its workflow must build that one example itself (scoped `cargo build -p … --example`), since
  artifacts do not cross workflows.
- **Ruleset**: `master` requires `Web tests`. When this PR lands, the requirement is replaced by the
  per-package check names. Done by the agent at merge time — doing it earlier would block every open PR,
  none of which carries the new checks.

### User Impact

- Contributors see one check per package and only pay for the packages they touched. A Rust-only PR
  runs no Cypress at all.
- A web PR no longer waits on `rust-build` unless it touches `tddy-livekit-web`.

## Implementation Plan

1. Detection script with unit tests (fixture workspaces, global inputs, transitive dependents).
2. Shared reusable workflow: build gate → affected decision → package job (install, build what the
   package needs, its test scripts, JUnit report).
3. One caller workflow per existing package; the rule test that pairs workspaces and workflows.
4. Delete the `web` job; run script tests in CI; update the CI guide.
5. At merge: edit the ruleset.

## Acceptance Criteria

- [ ] Each workspace package has exactly one `web-<package>.yml`, and the rule test enforces it.
- [ ] A PR touching only `packages/tddy-rpc-web` runs `tddy-rpc-web`'s and its dependents' tests and
      skips every other package's.
- [ ] A PR touching `bun.lock` runs every package.
- [ ] A skipped package's check reports success, not "expected — waiting".
- [ ] `ci.yml` no longer has a `web` job; the Rust jobs are unchanged.
- [ ] The CI scripts' tests run in CI.
- [ ] The ruleset requires the per-package checks instead of `Web tests` once this lands.

## References

- [docs/dev/guides/ci.md](../../../dev/guides/ci.md) — updated at wrap.
- Backlog: [2026-10-05-ci-script-tests-are-not-run-in-ci.md](../../../dev/todo/2026-10-05-ci-script-tests-are-not-run-in-ci.md).
- Successor: `feature/web-split/react-commons` and every package node, which add their own workflow.
