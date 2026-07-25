# Changeset: Branch resolution (QueryBranch) + Start-Session remote branch & base label

**Date**: 2026-07-25
**Status**: 🚧 In Progress
**Type**: Feature

## Affected Packages

- **tddy-service**: [proto/connection.proto](../../../packages/tddy-service/proto/connection.proto) — new `QueryBranch` RPC + messages; `StartSessionRequest.create_remote_branch`.
- **tddy-core**: [worktree.rs](../../../packages/tddy-core/src/worktree.rs) — `push_new_branch_to_origin`, public `worktree_path_for_branch`.
- **tddy-daemon**: [connection_service.rs](../../../packages/tddy-daemon/src/connection_service.rs) — `query_branch` handler; thread `create_remote_branch` through session-start intent helpers.
- **tddy-web**: [components/sessions/prstack/](../../../packages/tddy-web/src/components/sessions/prstack/), [CreateSessionPane.tsx](../../../packages/tddy-web/src/components/sessions/CreateSessionPane.tsx), [utils/](../../../packages/tddy-web/src/utils/) — `useQueryBranch`, row rendering, checkbox, base-branch label, `deriveStackBaseBranch`.

## Related Feature Documentation

- [PRD-2026-07-25-branch-query-and-remote-branch.md](../../ft/coder/1-WIP/PRD-2026-07-25-branch-query-and-remote-branch.md)
- [pr-stack-live-status.md](../../ft/coder/pr-stack-live-status.md)
- [session-drawer.md](../../ft/web/session-drawer.md)

## Summary

Add a `QueryBranch` RPC that resolves the in-progress session, on-disk worktree, and live GitHub PR
status for a head branch, and render those on the PR-Stack "Planned PRs" rows. Add a pre-checked
"Create Remote Branch" checkbox to the Start-Session dialog that pushes the new branch to `origin`
at session start, and show the concrete base branch in the dialog's "New branch from base" option.

## Background

See the PRD's Background. Branch/worktree/session were never rendered (only an internal join key);
planned-PR branches are local-only (never pushed); and the "New branch from base" label never names
the actual base ref even though the daemon already resolves it.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **PRD Documentation**: PRD created (`docs/ft/coder/1-WIP/PRD-2026-07-25-*.md`)
- [x] **Changeset**: This document
- [ ] **Package Documentation**: Update `pr-stack-live-status.md`, `session-drawer.md`, package `changesets.md` (wrap step)
- [x] **Implementation**: proto + regen, daemon handler, worktree push, web hook/components/dialog
- [x] **Testing**: All acceptance + unit/integration tests passing (Cypress 10/10, bun 5/5, Rust push 1/1)
- [x] **Integration**: QueryBranch end-to-end from row render to daemon resolution
- [x] **Code Quality**: clippy clean (`-D warnings`), vite build ok, cypress green

## Technical Changes

### State A (Current)

- PR status resolved by branch via `GetPrStatus` + `usePrStatus`; session resolved client-side by
  `resolveNodeSession` (`node.branch === SessionEntry.branch`); worktree not surfaced.
- `PlannedPrRow` renders title/description, an in-progress badge (from `resolveNodeSession`), PR
  link/state (from `usePrStatus`), internal-status badge, Repoint, and the Start/status control.
- `StartSessionRequest` has no remote-branch flag; the daemon creates branches **locally only**
  (`git push` appears only in tests + the orchestrator agent's git-ops, never in `StartSession`).
- The dialog's branch option is a static `<option>New branch from base</option>`; the pane never
  receives a base branch.

### State B (Target)

- New `QueryBranch(session_id, branch)` RPC → `{ branch, session, worktree, pr }`. `useQueryBranch`
  polls it per rendered branch; rows render worktree + in-progress + PR from the resolution. The
  legacy `GetPrStatus` / `usePrStatus` / `resolveNodeSession` surfaces remain (additive).
- `StartSessionRequest.create_remote_branch` (field 28). Pre-checked checkbox on the dialog
  (new-branch mode); when set, the daemon `git push -u origin <branch>` after worktree creation and
  sets `Changeset.remote_pushed = true`.
- The dialog's new-branch option reads `New branch from base: <name>` — predecessor stack branch for
  a stack node (via `deriveStackBaseBranch`), project default branch otherwise.

### Delta (What's Changing)

#### tddy-service
- **Proto**: `QueryBranch` RPC; `QueryBranchRequest`/`QueryBranchResponse`; `BranchResolution`,
  `BranchSession`, `BranchWorktree` (reuse `PrStatusView`); `StartSessionRequest.create_remote_branch = 28`.

#### tddy-core
- **worktree.rs**: `push_new_branch_to_origin(worktree_dir, branch) -> Result<(), String>`
  (`git push -u origin <branch>` via `git_remote_command`); public `worktree_path_for_branch(repo_root, branch) -> Option<PathBuf>`.

#### tddy-daemon
- **connection_service.rs**: `query_branch` handler (reuses `get_pr_status` prologue + `get_pr_by_head`
  + branch→session scan + `worktree_path_for_branch`). Thread `create_remote_branch` through the
  three session-start intent helpers → push after `create_worktree_with_retry` → set `remote_pushed`.

#### tddy-web
- **usePrStatus.ts sibling** `useQueryBranch.ts`: per-branch poll → `Record<string, BranchResolution>`.
- **PlannedPrRow.tsx / PlannedPrList.tsx / PrStackScreen.tsx**: render worktree/in-progress/PR from the
  resolution; new testids `pr-stack-worktree-<nodeId>`, `pr-stack-session-<nodeId>`.
- **CreateSessionPane.tsx**: `createRemoteBranch` state + checkbox; `baseBranchLabel` in the option;
  `CreateSessionInitialValues` gains `createRemoteBranch`, `baseBranchLabel`.
- **utils/deriveStackBaseBranch.ts**: pure base-branch derivation for a stack node.

## Implementation Milestones

- [x] Proto + Rust/TS codegen for `QueryBranch` and `create_remote_branch`
- [x] `tddy-core` worktree push + `worktree_path_for_branch`
- [x] Daemon `query_branch` handler + `create_remote_branch` wiring
- [x] Web `useQueryBranch` + row rendering
- [x] Web checkbox + base-branch label + `deriveStackBaseBranch`
- [ ] Docs wrapped

## Testing Plan

### Testing Strategy

- **Web (component/acceptance)** — Cypress with `mountWithRpc` + `anInMemoryRpcBackend`
  (`aSessionsDrawerBackend`), the established pattern for the PR-Stack screen and the Start-Session
  dialog. Drives the whole flow (drawer → screen/dialog → stubbed RPC) and asserts rendered fields
  and captured request params (`backend.callsTo(...)`).
- **Web (unit)** — `bun:test` for the pure `deriveStackBaseBranch` helper.
- **Rust (unit)** — `#[test]` in `worktree.rs` for `push_new_branch_to_origin` against a real bare
  `origin` fixture (matching the existing worktree tests), asserting the branch appears on the remote.

Rationale: the QueryBranch *display* and the checkbox/base-label are UI-integration behaviors best
pinned by Cypress acceptance tests; base-branch derivation and the remote push are isolated logic
best pinned by unit tests.

## Acceptance Tests

### tddy-web
- [x] **Acceptance (Cypress)**: `PrStackBranchQueryAcceptance.cy.tsx` — a planned-PR row renders the
  worktree indicator, in-progress badge, and PR link/state from `QueryBranch`; updates on the poll
  interval; shows nothing extra when the branch resolves to nothing. (6/6)
- [x] **Acceptance (Cypress)**: `CreateSessionCreateRemoteBranchAcceptance.cy.tsx` — the checkbox is
  pre-checked; submitting sends `createRemoteBranch = true`; unchecking sends `false`. (3/3)
- [x] **Acceptance (Cypress)**: `PrStackStartSessionBaseBranchAcceptance.cy.tsx` — the Start-session
  CTA opens the dialog with "New branch from base: `<predecessor branch>`". (1/1)
- [x] **Unit (bun)**: `deriveStackBaseBranch.test.ts` — predecessor branch for a dependent node,
  default branch for a root, nearest non-merged ancestor when a parent is merged. (5/5)

### tddy-core
- [x] **Unit (Rust)**: `worktree.rs::push_new_branch_to_origin` — pushes a new local branch to a bare
  `origin` and sets upstream; the branch is listed on the remote afterward. (1/1)

## Technical Debt & Production Readiness

- [ ] Per-branch polling issues N calls per refresh (matches `usePrStatus`); a batch variant is a
  possible future optimization (out of scope — operator chose the additive/per-branch shape).
- [ ] `QueryBranch` and `GetPrStatus` both call `get_pr_by_head` for the same branch while both hooks
  run; acceptable under the additive decision, revisit if polling churn matters.

## Decisions & Trade-offs

- **Additive, not consolidating** (operator decision): keep `GetPrStatus` / `usePrStatus` /
  `resolveNodeSession`; add `QueryBranch` alongside. Smaller blast radius; some overlap in PR lookup.
- **Per-branch QueryBranch** (default, pending operator confirmation): mirrors `usePrStatus`; simpler
  handler than a batch map.
- **Push at session start** (default, pending confirmation): the remote branch exists immediately
  even before commits; `remote_pushed` recorded. No deferred-push machinery.
- **Base label everywhere** (default, pending confirmation): stack CTA shows the predecessor branch,
  general dialog shows the project default; derived client-side.

## References

- Source investigation: session `dab951e0` (branch/worktree/session not rendered; branches local-only).
- [changeset-doc.mdc](../../../.cursor/rules/changeset-doc.mdc)
