# Changeset: Per-package web PR checks

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — CI contract change across 7 workspace packages (new reusable workflow + one caller each), a detection script with unit tests, ruleset change at merge

**Stack**: `#web-split` 1/16 — branch `feature/web-split/web-ci`, base `master`

**PR**: [#838](https://github.com/uppin/tddy-coder/pull/838)

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

- [x] Detection script + unit tests
- [x] Reusable workflow + 7 caller workflows
- [x] Workspace ↔ workflow rule test
- [x] Delete `ci.yml` `web` job; run CI script tests in CI
- [x] CI guide updated
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
Cypress specs, `bun install --frozen-lockfile`, the web bundle when `needs-web-bundle`, its `test:ci`
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

- [x] `affectedPackages` passes its unit tests
- [x] Rule test passes against the 7 callers
- [x] `web` job deleted; script tests step present
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
- **Required-check name is `web / <package> tests`, not `<package> tests`.** GitHub shows a called
  workflow's jobs as `<calling job id> / <job name>` (compare `gate / Draft build gate` on master). The
  caller's job id is `web`, so the ruleset contexts are `web / <package> tests`. A context without the
  prefix never reports, and a required check that never reports blocks the merge. Confirm the names on
  the first CI run of a ready PR, before the ruleset edit.
- **A failed `changes` job fails the check, it does not skip it.** A skipped job reports success, so
  `if: needs.changes.outputs.affected == 'true'` alone would pass every package when the detector breaks.
  The test job's `if` also runs when `changes` failed, and its first step fails.
- **Inputs differ from the plan in two places.** `needs-web-bundle` replaces a generic "run the package's
  build" input, because tddy-web is the only package whose tests need its own build. `needs-tddy-coder`
  was not planned: `tddy-rust-typescript-tests`' only test spawns `target/debug/tddy-coder` and serves
  `packages/tddy-web/dist`, so without both its `test:ci` fails in `beforeAll`. It is wired, not left out.
- **A caller's own edit affects its package.** `.github/workflows/web-<package>.yml` counts toward that
  package's affected set, so editing a caller re-runs the check it configures. Not in the planned rule list.
- **`concurrency` is set in each caller**, group `web-<package>-<ref>`, not in `web-package.yml`, so a
  package's runs cancel only their own predecessors. Unverified on GitHub until the first run.
- **Packages with nothing to run.** `tddy-desktop` and `tddy-connectrpc-testkit` have no `test:ci` and no
  build of their own, so their checks install the workspace and pass. `tddy-desktop`'s front end is
  tddy-web's bundle (`frontendDist` in `src-tauri/tauri.conf.json`), which its `package.json` does not
  depend on: a tddy-web change does not run the desktop check. Follow-up, not in this node.
- **Cost.** No Rust cache in these jobs. `tddy-rust-typescript-tests` cold-builds `tddy-coder-cli`, and
  `tddy-livekit-web` builds `echo_server`. Global inputs run every check, so a `package.json` or `bun.lock`
  change builds tddy-coder in that check too. Locally, `tddy-coder-cli` took ~2 min with deps partly warm.
- **Test additions, no test file changed.** `test:ci` added to tddy-web (alias of `test:unit:ci`),
  tddy-livekit-web (its five bun test files, 44 tests that no script ran before) and
  tddy-rust-typescript-tests.

## Refactoring Needed

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

Scoped, run in this worktree (`/Users/mantasi/.superset/worktrees/tddy-coder/yolo-web-split-web-ci`):

- `./dev bun test` on the five CI script tests (`web-affected`, `web-workflows`, `nextest-serial-groups`,
  `ci-e2e-timing`, `livekit-ci-server`): 69 pass, 0 fail.
- `./dev bun run --filter tddy-web test:ci`: 1336 pass. `--filter tddy-livekit-web test:ci`: 44 pass.
  `--filter tddy-rust-typescript-tests test:ci`: 1 pass, after the binary and bundle below.
- `./dev bun run --filter tddy-web build`: exit 0. `./dev cargo build --locked -p tddy-coder-cli --bin tddy-coder`
  and `-p tddy-livekit --example echo_server`: exit 0.
- All `.github/workflows/*.yml` parse as YAML. `actionlint` is not installed; the workflows are proven by
  this PR's own CI run, as the testing plan says.
- Not run: the Cypress component suites of tddy-web and tddy-livekit-web (about 50 min and Docker).

Flakes seen, not fixed here (no test file changed in this node):

- `nextest-serial-groups.test.ts` › `no_binary_that_starts_the_livekit_testkit_is_in_a_serial_test_group`
  hit bun's 5 s default timeout once (5.0 s) on a cold run. It passes alone in 2.2 s and in the rerun.
  It runs in CI now, so an explicit timeout in that test is the fix.
- `echo-unary.test.ts` `beforeAll` hit bun's 5 s hook timeout on the first launch of a freshly built
  `tddy-coder` (macOS). Three passes since, about 0.3 s each. Its own readiness wait is 10 s, longer than
  the hook's limit.

Left alone, not from this change:

- `packages/tddy-workflow-recipes/build.rs` rewrites `generated/schema-manifest.json` on every build (key
  order, drops `version`), so the committed copy is stale. Reverted, not committed.
- `packages/tddy-web/src/buildId.ts` is rewritten by the prebuild step. Reverted, not committed.
- `scripts/local-package-cache.test.ts`, `nextest-windowed.test.ts` and `nextest-e2e-archive.test.ts` are
  not run in CI either, and no todo entry names them.
- `rust-build` still stages `echo_server` into `rust-fixture-bins`, and no job reads it now.

## TODO

- [x] Record initial discovery (`2026-11-07-per-package-web-checks-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer waived the per-node review for this stack, 2026-11-07: "finish the rest without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched tests (`./dev bun test ./scripts/web-affected.test.ts ./scripts/web-workflows.test.ts`) — verify 100% pass
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
