# Changeset: Per-package web PR checks

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — CI contract change across 7 workspace packages (new reusable workflow + one caller each), a detection script with unit tests, ruleset change at merge

**Stack**: `#web-split` 1/16 — branch `feature/web-split/web-ci`, base `master`

## Initial Discovery

[2026-11-07-per-package-web-checks-initial-discovery.md](./2026-11-07-per-package-web-checks-initial-discovery.md)

## Prerequisites

Open items in [`docs/dev/todo/`](../todo/) this change runs into.

### ✅ RESOLVED HERE — The CI scripts' own tests are not run by CI — [`2026-10-05-ci-script-tests-are-not-run-in-ci.md`](../todo/2026-10-05-ci-script-tests-are-not-run-in-ci.md)

This node adds a script test of its own (`scripts/web-affected.test.ts`) that must run in CI. The step
that runs it also runs the three script tests the entry names, and the "run locally" caveats in
`docs/dev/guides/ci.md` and `.config/nextest.toml` are dropped.

### ⚠ DURING — `Web tests` run LiveKit from the floating `:master` image — [`2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md`](../todo/2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md)

The `docker pull livekit/livekit-server:master` step moves from `ci.yml` into the LiveKit package's
workflow unchanged. Recorded, not fixed here: changing the server version is its own risk.

### ⚠ DURING — Cypress runs without the React Compiler — [`2026-10-23-cypress-runs-without-the-react-compiler.md`](../todo/2026-10-23-cypress-runs-without-the-react-compiler.md)

The package workflows run the same `cypress:component:ci` scripts, so the gap is neither closed nor
widened.

## Affected Packages

- **CI** (`.github/workflows/`): new `web-package.yml` (reusable) and `web-<package>.yml` per package;
  `ci.yml` loses its `web` job.
- **scripts**: new `scripts/web-affected.ts` + `scripts/web-affected.test.ts`.
- **tddy-web**, **tddy-livekit-web**, **tddy-rpc-web**, **tddy-tauri-web**, **tddy-desktop**,
  **tddy-rust-typescript-tests**, **tddy-connectrpc-testkit**: a `test:ci` script where one is missing
  (no test code changes).
- **docs/dev/guides/ci.md** — updated at wrap.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-per-package-web-checks.md](../../ft/web/1-WIP/PRD-2026-11-07-per-package-web-checks.md)

## Summary

Replace the single `Web tests` job with one workflow per JS workspace package, each of which tests
its package only when the PR affects it.

## Background

See the PRD. The stack needs the mechanism in place before it creates a dozen new packages.

## Responsibility

- `scripts/web-affected.ts`: `affectedPackages(changedFiles, workspace) → Set<string>`, the workspace
  reader (root `workspaces` + each `package.json`'s `workspace:*` deps), the reverse-dependency
  closure, the **global inputs** list, and a CLI (`--package <name>` → prints `affected=true|false`
  for `$GITHUB_OUTPUT`; `--base`/`--head` for the diff).
- `.github/workflows/web-package.yml` (`workflow_call`, input `package`, optional inputs
  `needs-rust-fixture`, `needs-livekit-image`): build gate → `changes` job → test job named
  **`<package> tests`** (`if: needs.changes.outputs.affected == 'true'`), JUnit report under a
  *different* check name (`<package> report`) so the job-level check is the one required.
- `web-<package>.yml` for each of the 7 workspace packages.
- The rule test pairing `workspaces` with `web-*.yml` callers.
- Deleting `ci.yml` `web`; running the CI scripts' tests.
- Changing the `master` ruleset (`Web tests` → the 7 `<package> tests` checks) **at merge**.

## Boundaries

- No test code in any package changes; no Rust CI change beyond deleting `web` (and the
  `rust-fixture-bins` upload stays — `rust-test` uses it).
- No new package is created here; each later node adds its own `web-<pkg>.yml`.
- Not a matrix in one workflow: the developer asked for a separate workflow per package.

## Dependencies

Root node: no parent PRs.

## Draft PR contract

The first push of this PR (commit 2): `scripts/web-affected.ts` exporting `affectedPackages`,
`readWorkspace`, `GLOBAL_INPUTS` with bodies `// TODO(web-ci): implement`, and failing tests:

- `scripts/web-affected.test.ts` — the detection cases below;
- `scripts/web-workflows.test.ts` — the workspace ↔ workflow pairing rule.

## Green wave

**Wave:** 1 of 9
**Greenable independently:** yes — the tests drive the script over fixture workspaces and read the
repo's `.github/workflows/` files; nothing from another node.
**Concurrent with:** none (only node in wave 1).
**Blocks:** every other node (each adds a `web-<pkg>.yml` calling `web-package.yml`).

Real dependency edges, as opposed to the branch line:

    n1 → all      n2 → n5      n3,n4,n5 → n6      n3,n5 → n7,n8,n9
    n6 → n10,n11,n13      n10 → n12,n13      n7 → n13
    n6,n7,n8,n10,n11,n12,n13 → n14      component packages → n15      all → n16

## Scope

- [ ] Detection script + unit tests
- [ ] Reusable workflow + 7 caller workflows
- [ ] Workspace ↔ workflow rule test
- [ ] Delete `ci.yml` `web` job; run CI script tests in CI
- [ ] CI guide updated
- [ ] Ruleset: `Web tests` replaced by the per-package checks (at merge)

## Technical Changes

### State A (Current)

`ci.yml` `web` ("Web tests", `needs: rust-build`, 90 min timeout) builds the web bundle and runs
`tddy-web` unit + Cypress component, `tddy-rpc-web`/`tddy-tauri-web` unit, `tddy-livekit-web`
Cypress (needs the `echo_server` example from `rust-fixture-bins` and a pulled LiveKit image), one
JUnit check "Web tests". The ruleset requires it. `build-gate.yml` is reusable and outputs `build`.
`tddy-desktop`, `tddy-rust-typescript-tests` and `tddy-connectrpc-testkit` are not tested in CI.

### State B (Target)

`web-<package>.yml` per package → `web-package.yml`: `gate` (build-gate) → `changes`
(`bun scripts/web-affected.ts --package <p> --base <pr base sha> --head <sha>`; pushes/dispatch →
`affected=true`) → `<package> tests` (only if affected): nix, Cypress libs when the package has
Cypress specs, `bun install --frozen-lockfile`, the package's `build` if it has one, its `test:ci`
and `cypress:component:ci`, JUnit report check `<package> report`. `tddy-livekit-web`'s caller sets
`needs-rust-fixture` (scoped `cargo build -p <crate> --example echo_server`) and
`needs-livekit-image`. A package with no tests runs its build only (`tddy-desktop`: front-end build
only, no `tauri build`).

### Delta

- `scripts/web-affected.ts`, `scripts/web-affected.test.ts`, `scripts/web-workflows.test.ts` (new).
- `.github/workflows/web-package.yml` + 7 callers (new); `ci.yml` (`web` job removed; a script-tests
  step added to the `generated-code` job, which already installs bun).
- `package.json` of packages lacking `test:ci` (added, pointing at existing tests).

## Implementation Milestones

- [ ] `affectedPackages` passes its unit tests
- [ ] Rule test passes against the 7 callers
- [ ] `web` job deleted; script tests step present
- [ ] A PR run shows 7 `<package> tests` checks, unaffected ones skipped-success

## Testing Plan

**Level:** unit (`bun test`) for the script and the rule; the workflows themselves are proven by this
PR's own CI run (once ready) — a CI definition cannot be exercised locally.

## Acceptance Tests

### scripts (`bun test ./scripts/web-affected.test.ts ./scripts/web-workflows.test.ts`)

- `a change inside one package affects that package` — `scripts/web-affected.test.ts`
- `a change in a package affects every package that depends on it, transitively`
- `a change in a dependent does not affect its dependency`
- `a change outside every package affects nothing`
- `a change to a global input affects every package` (root `package.json`, `bun.lock`,
  `.github/workflows/web-package.yml`, `scripts/web-affected.ts`, `flake.nix`, `flake.lock`)
- `devDependencies on a workspace package count as dependencies`
- `the CLI prints affected=true for an affected package and affected=false otherwise`
- `every workspace package has exactly one web workflow` — `scripts/web-workflows.test.ts`
- `every web workflow names a workspace package and calls the shared workflow`

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Job-level check is the required one, JUnit check renamed.** A skipped job reports success; a
  JUnit report from a job that never ran reports nothing, which would leave a required check pending.
- **Ruleset edited at merge, by the agent** (developer's choice). Doing it earlier blocks every open PR.
- **`tddy-livekit-web` builds its own fixture** — artifacts do not cross workflows; a scoped example
  build is the cost of not waiting on `rust-build`.
- **Global inputs run everything** — a lockfile or toolchain change can break any package.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

## TODO

- [x] Record initial discovery (`2026-11-07-per-package-web-checks-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched tests (`./dev bun test ./scripts/web-affected.test.ts ./scripts/web-workflows.test.ts`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
