# Changeset: PR-Stack live status & repoint

**PRD**: `docs/ft/coder/pr-stack-live-status.md`
**Branch**: `feat-pr-stack-status`

## Summary

Make the remote branch name the durable link between a Planned PR, its worktree/session, and its
GitHub PR. Assign a definitive branch at creation; resolve the in-progress session by branch in the
web; poll GitHub for the PR number/link/state; add a web Repoint control; and fix child-session
spawning so a node is branched off its parent's branch (the effective base) instead of the default
branch, refusing to start before a non-merged parent is started.

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Write acceptance tests (Cypress + Rust) — verified failing
- [x] Write unit/integration tests (red phase) — verified failing
- [x] Proto surface added + regenerated (TS via buf, Rust via build.rs)
- [x] Implement: daemon `get_pr_status` + `repoint_planned_pr` handlers (real, no stubs)
- [x] Implement: `StackNode.branch` at creation (both plan + add paths)
- [x] Implement: `Stack::base_ref_for_spawn` (effective base + ordering guard)
- [x] Implement: `GithubPrApi::get_pr_by_head` (+ `PrView`/`PrState`/`pr_state_from_github`)
- [x] Implement: `pr_stack::repoint_planned_pr_node`
- [x] Implement: daemon `resolve_chain_base_ref` fix (capability 5) + call sites
- [x] Implement: `SessionEntry.branch` enrichment
- [x] Implement: web `resolveNodeSession`, `usePrStatus`, `PrStackScreen`/`PlannedPrRow`/`PlannedPrList` UI
- [x] Regenerate `connection_pb.ts`

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-web/cypress/component/PrStackLiveStatusAcceptance.cy.tsx` | Cypress acceptance: in-progress, PR link/state, polling, Repoint |
| `packages/tddy-web/src/components/sessions/prstack/nodeSession.ts` | `resolveNodeSession(node, sessions)` branch→session resolver |
| `packages/tddy-web/src/components/sessions/prstack/nodeSession.test.ts` | Unit tests for `resolveNodeSession` |
| `packages/tddy-web/src/components/sessions/prstack/usePrStatus.ts` | Poll `GetPrStatus` per branch → `branch → PrStatusView` map |
| `packages/tddy-core/tests/pr_stack_spawn_base_acceptance.rs` | Acceptance: `Stack::base_ref_for_spawn` sequence/guard behavior |
| `packages/tddy-workflow-recipes/tests/pr_stack_live_status_acceptance.rs` | Acceptance: branch-at-creation, repoint node, PR state |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | `SessionEntry.branch = 28`; `PrStatusView`; `GetPrStatus`/`RepointPlannedPr` RPCs + messages |
| `packages/tddy-core/src/changeset.rs` | `Stack::base_ref_for_spawn`; keep `StackNode.branch` semantics (now set at creation) |
| `packages/tddy-workflow-recipes/src/plan_pr_stack/mod.rs` | `planned_prs_into_stack_nodes` sets `branch` from `branch_suggestion` |
| `packages/tddy-workflow-recipes/src/pr_stack/mod.rs` | `add_planned_pr_node` sets `branch` at creation; `repoint_planned_pr_node` |
| `packages/tddy-workflow-recipes/src/orchestrate_pr_stack/github.rs` | `GithubPrApi::get_pr_by_head` + `PrView`/`PrState`; impl on `RealGithubPrApi` |
| `packages/tddy-daemon/src/connection_service.rs` | `resolve_chain_base_ref` uses `base_ref_for_spawn`; `get_pr_status` + `repoint_planned_pr` handlers |
| `packages/tddy-daemon/src/connection_tonic_adapter.rs` | Adapter methods for the two new RPCs |
| `packages/tddy-daemon/src/session_list_enrichment.rs` | Populate `SessionEntry.branch` from `Changeset.branch` |
| `packages/tddy-web/src/components/sessions/prstack/PrStackScreen.tsx` | `sessions` prop; wire `usePrStatus` + repoint handler |
| `packages/tddy-web/src/components/sessions/prstack/PlannedPrRow.tsx` | In-progress indicator, PR link + state, Repoint control |
| `packages/tddy-web/src/components/sessions/prstack/PlannedPrList.tsx` | Thread session-resolution + status props to rows |
| `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx` | Pass full sessions list into `PrStackScreen` |
| `packages/tddy-web/cypress/support/testIds.ts` | New test ids: in-progress, PR link, PR state, Repoint button |
| `packages/tddy-web/cypress/support/pages/prStackScreenPage.ts` | Page-object helpers for the new row affordances |
| `packages/tddy-web/cypress/support/rpc/prStackFixtures.ts` | `aSessionWithBranch`, `aPrStatus` fixtures |
| `packages/tddy-web/cypress/support/rpc/prStackBackend.ts` | Backend stubs for `getPrStatus`/`repointPlannedPr` (new small helper) |

## Design decisions

### D1 — `StackNode.branch` is the link key, set at creation
`add_planned_pr_node` and `planned_prs_into_stack_nodes` set `branch = Some(<canonical>)` derived
from `branch_suggestion`. `branch_suggestion` is retained as the derivation source only. Nothing
keys off `branch == None` for "unspawned" — `session_id` is that signal — so promoting the field is
safe.

### D2 — Session resolution by branch, in the frontend
A new `SessionEntry.branch` (proto field 28, enriched from `Changeset.branch`) lets the web match
`node.branch === session.branch`. Prefer the active session; tie-break by most recent `updatedAt`.

### D3 — `GetPrStatus(branch)`, polled
Daemon resolves `owner/repo` from the orchestrator session's repo remote and calls
`get_pr_by_head`. Token-gated: no token → `exists = false`, no error. The web polls on a fixed
interval and renders `#<number>` (link) + state.

### D4 — Repoint = DAG + rebase + GitHub base
`repoint_planned_pr_node` drops merged parents, computes the effective base, rebases the local
branch (skipped when the branch isn't local), force-pushes, and patches the PR base — the same
primitives as `bridge::execute_stack_repoint`, applied to a single node.

### D5 — Spawn base resolved in the daemon
`resolve_chain_base_ref`, extended to take the new branch name, resolves the pr-stack node by
`branch == new_branch_name` and returns `Stack::base_ref_for_spawn(node_id, default)`. Both the web
Start-session and agent `spawn-child` paths funnel through `spawn_claude_cli_session_inner`, so the
fix lands once.

### D6 — Ordering guard
`base_ref_for_spawn` errors when a non-merged parent has no `session_id` (un-started), naming the
parent. Merged parents are skipped, not required. Multi-parent nodes use the nearest ancestor as
the single base (octopus merge base is a documented non-goal).

## Acceptance tests

### Cypress — `packages/tddy-web/cypress/component/PrStackLiveStatusAcceptance.cy.tsx`
1. `marks a planned-PR row in progress when a live session owns its branch` — node branch
   `feature/x/n1` + an active session with `branch: "feature/x/n1"` → in-progress indicator shown.
2. `shows no in-progress indicator when no session owns the node branch` — no matching session →
   the Start-session CTA remains, no in-progress indicator.
3. `shows the GitHub PR number as a link to the PR for the node branch` — `getPrStatus` →
   `{ exists, number: 42, url }` → row shows `#42` linking to the PR url.
4. `shows the GitHub PR state reported for the node branch` — `getPrStatus` → `state: "merged"` →
   row shows the merged state.
5. `updates the PR status on the polling interval without user action` — `getPrStatus` returns
   `open` then `merged` on successive calls; after the interval the row shows `merged`.
6. `shows a Repoint control on a node whose predecessor has merged` — parent `pr_status.phase`
   `merged` → Repoint control visible on the dependent row.
7. `hides the Repoint control when no predecessor has merged` — all parents open → no control.
8. `repoints the node via RepointPlannedPr and updates the row from the response` — clicking
   Repoint calls `repointPlannedPr({ nodeId })` and the row re-renders from the returned stack.

### Rust — `packages/tddy-core/tests/pr_stack_spawn_base_acceptance.rs`
9. `bases a node on its non-merged parent branch` — `base_ref_for_spawn("n2", "origin/master")` with
   parent `n1` (branch `feature/x/n1`, started) → `origin/feature/x/n1`.
10. `bases a root node on the stack default branch` — node with no parents → `origin/master`.
11. `bases a node on the stack default when its only parent is merged` — parent `n1` merged →
    `origin/master`.
12. `refuses to resolve a base when a non-merged parent is unstarted` — parent `n1` open and
    `session_id = None` → Err naming `n1`.

### Rust — `packages/tddy-workflow-recipes/tests/pr_stack_live_status_acceptance.rs`
13. `add_planned_pr_node assigns a definitive branch at creation` — new node has `branch = Some(...)`.
14. `planned_prs_into_stack_nodes assigns branch from the branch suggestion` — converted node has
    `branch == branch_suggestion`.
15. `repoint_planned_pr_node drops a merged parent from the node parents` — after repoint the merged
    parent is gone from `parents`.
16. `repoint_planned_pr_node retargets the open PR base to the effective base` — fake `GithubPrApi`
    records `patch_pr_base(number, "master")` (remote-only branch → local rebase skipped).
17. `get_pr_by_head reports a merged PR as merged` — fake `GithubPrApi` returns `PrState::Merged`.

## Unit tests (red phase)

### Web — `packages/tddy-web/src/components/sessions/prstack/nodeSession.test.ts`
1. `resolves the live session whose branch matches the node branch`
2. `returns undefined when no session branch matches the node branch`
3. `prefers the active session when multiple sessions share the branch`

### Rust — colocated `#[cfg(test)] mod tests`
4. `changeset.rs` — `base_ref_for_spawn` unit coverage mirroring acceptance 9–12 at the type level.
5. `github.rs` — `get_pr_by_head` derives `PrState` from GitHub `state`/`merged_at` (open, merged,
   closed, draft) via a fake response mapper.
6. `pr_stack/mod.rs` — `add_planned_pr_node` sets `branch` when a `branch_suggestion` is supplied and
   derives one when it is absent.

### Rust — daemon
7. `session_list_enrichment.rs` — `SessionEntry.branch` is populated from `Changeset.branch`.
8. `connection_service.rs` (`chain_base_resolution_tests`) — `resolve_chain_base_ref` returns the
   parent-node effective base for a pr-stack orchestrator parent when the new branch matches a node
   (replaces the prior `returns_none_for_a_branchless_pr_stack_orchestrator_parent` expectation for
   the node-matched case; the generic non-matching case still returns `None`).

## Validation Results (pr-wrap)

- **validate-changes**: 0 critical, 0 warning. Reviewed the full production diff (core `base_ref_for_spawn`,
  `github` PR-state + `get_pr_by_head`, `repoint_planned_pr_node`, daemon handlers +
  `resolve_chain_base_ref` fix + enrichment, web hooks/UI). Real error propagation throughout;
  network/git run inside `spawn_blocking`; auth mirrors `add_planned_pr`; no unjustified `unwrap`
  in production paths. INFO: repoint availability derives from the stored stack `pr_status`
  (`is_skipped`), not the live poll — matches the PRD contract.
- **validate-prod-ready**: PASS. No `unimplemented!`/`todo!`/`dbg!`/`println!`/`console.log` in
  production code; no `TODO(pr-stack…)`/`FIXME(pr-stack…)` left (the daemon RPC stubs were
  replaced with real handlers). No "red/green phase" leakage.
- **validate-tests**: PASS. Fluent Given/When/Then, one behavior per test, named builders, exact
  assertions, deterministic polling via `cy.clock`/`cy.tick`.
- **Lint**: `cargo fmt --check` clean; `cargo clippy -p tddy-core -p tddy-workflow-recipes
  -p tddy-daemon --all-targets -- -D warnings` — see PR summary.

## Notes

- **Behavior change to an existing test.** `resolve_chain_base_ref_returns_none_for_a_branchless_pr_stack_orchestrator_parent`
  (`connection_service.rs`) encodes the current buggy behavior for the node-matched case and is
  updated by test #8 above.
- **Non-goal.** Multi-parent octopus/merge base; PR status caching beyond the fixed poll interval.
