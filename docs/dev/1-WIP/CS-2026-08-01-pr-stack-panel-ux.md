# Changeset: PR-Stack panel UX

**Created:** 2026-08-01
**Status:** Complete
**PRD:** [docs/ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md](../../ft/coder/1-WIP/PRD-2026-08-01-pr-stack-panel-ux.md)

## Affected Packages

- [x] `tddy-service` — proto: `QueryBranchRequest.base_branch`, `BranchResolution.base_sync`,
      `BranchWorktree.dirty`/`dirty_paths`, new `BranchBaseSync`, `PullBaseIntoBranch*`, `ReorderPlannedPr*`
- [x] `tddy-core` — `StackNode.display_order`, `Stack::display_order()`, new `base_sync` module
- [x] `tddy-workflow-recipes` — order assignment + `move_planned_pr_node`, `pull_base_into_node_branch`,
      new git primitives
- [x] `tddy-daemon` — base-sync and dirty legs on `query_branch`; `pull_base_into_branch` and
      `reorder_planned_pr` handlers; base-sync cache
- [x] `tddy-web` — expandable rows, session navigation, persisted ordering, base-sync badges, pull controls

## State A (Current)

**Ordering.** `Stack` has no ordering primitive. `PlannedPrList.tsx:48` sorts by `topoSortStackNodes`, a
stable Kahn sort — but a plan re-seed rewrites `Stack.nodes` wholesale and merge/repoint/`pr_set_parents`
rewrite `parents`, so the derived order moves for reasons unrelated to what the operator is looking at.
`pr_stack/mod.rs:86` records the current stance: order *is* the parent graph.

**The row.** `PlannedPrRow.tsx:107-235` renders every field unconditionally in one flex block, then a badge
strip, then a CTA slot. There is no expansion anywhere in the panel. The CTA slot's spawned branch is a plain
`<span>` (`:217`); `node.sessionId` is read only by `isNodeOrphaned`, and `resolution.session.sessionId`
is never read at all. `resolveNodeSession` is called in `PlannedPrList.tsx:58` but only `.isActive` is used.

**Base state.** `QueryBranch` (`connection_service.rs:7859`) resolves four legs per branch — session,
worktree, remote, PR — each non-erroring, on a 5s poll. None of them compares the branch to a base. The
request carries no base. `BranchWorktree` reports existence and path but not dirtiness. `has-conflicts` is an
`internal_status` kind that `internal_status.rs:36` never derives; the only detector,
`pr_actions.rs:76 pr_resolve_conflicts_action`, mutates the index and working tree. There is no commit
counting anywhere in the workspace (`rev-list --count` appears nowhere in `src/`).

**Taking base changes.** No surface for it. `RepointPlannedPr` changes the base and drops parent edges;
`sync_feature_with_origin_main` belongs to the merge-pr recipe and runs inside a session's own workflow.

## State B (Target)

**Ordering.** `StackNode.display_order: Option<u32>` is persisted; `Stack::display_order()` renders by it and
falls back wholesale to `topo_order()` for a plan that carries none. Every stack write numbers any unnumbered
node. `ReorderPlannedPr` moves one node up or down.

**The row.** Three regions: a summary header (toggle + badges + CTA), a hidden-not-unmounted detail body, and
an always-visible warning/error footer. The spawned indicator wraps in a button that selects and attaches the
bound child session.

**Base state.** `QueryBranch` gains a fifth leg. Given a base branch, it reports behind/ahead counts, a
conflict flag with paths, and an explicit `unavailable` discriminator — computed with `git rev-list
--left-right --count` and `git merge-tree --write-tree --name-only`, which touch no index, worktree, ref or
network. Results are cached on `(repo_root, base_ref, base_sha, head_ref, head_sha)`, a key that cannot go
stale — the ref *names* are in it because the cached answer carries them, and two node branches sitting at the
same commit (the normal state right after both are cut from the base) would otherwise render each other's
identity. `BranchWorktree` also reports dirtiness, outside that cache.

**Taking base changes.** `PullBaseIntoBranch` merges (default) or rebases the base into a node's branch inside
that node's own worktree, then pushes, and returns a fresh `BranchResolution` so the row repaints immediately.
The worktree it writes in must have that very branch checked out: the resolver also answers with one that
merely shares the branch's tip commit, which would land the base on a sibling's branch and then report a push
of a branch that never moved. A dirty worktree is refused unless the caller opts into committing and pushing
first. A conflict aborts and reports the paths.

## Delta

### New

- `packages/tddy-core/src/base_sync.rs` — `resolve_base_sync_refs`, `compare_base_sync_refs`,
  `branch_base_sync`, `BranchBaseSync`, `BaseSyncRefs`
- `packages/tddy-daemon/src/base_sync_cache.rs` — content-keyed, capped, caches errors too
- `tddy-workflow-recipes`: `assign_missing_display_order`, `move_planned_pr_node`,
  `pull_base_into_node_branch`, and git primitives `fetch_ref`, `worktree_is_clean`, `commit_all_tracked`,
  `merge_ref_into_worktree`, `rebase_branch_onto_ref`, `SyncOutcome`
- Proto: `BranchBaseSync`, `PullBaseIntoBranchRequest`/`Response`, `ReorderPlannedPrRequest`/`Response`,
  and the two RPCs
- `tddy-web` pure modules: `orderStackNodes.ts`, `parentTitles.ts`, `boundChildSession.ts`,
  `branchQueries.ts`, `baseSyncStatus.ts`

### Modified

- `StackNode` — `display_order` field, `Default` derive (~60 exhaustive literals across 19 files)
- `Stack` — `display_order()` beside `topo_order()`
- All seven `pr_stack` mutators + `bridge.rs` seeding — number unnumbered nodes on write
- `planned_prs_into_stack_nodes` — number from the plan's array index
- `query_branch` — base-sync leg, worktree dirtiness
- `QueryBranchRequest` (+`base_branch`), `BranchResolution` (+`base_sync`), `BranchWorktree`
  (+`dirty`, `dirty_paths`)
- `PlannedPrRow`, `PlannedPrList`, `PrStackScreen`, `stackPlan.ts`, `useQueryBranch.ts`,
  `AddPlannedPrForm.tsx`, `workflowViews.tsx`, `SessionMainPane.tsx`
- Test support: `testIds.ts`, `prStackScreenPage.ts`, `prStackFixtures.ts`, `src/test-utils/builders.ts`

### Removed

Nothing. Every existing test id, prop and RPC keeps its meaning; `topoSortStackNodes` stays as the
legacy-plan fallback.

## Milestones

### Milestone 1: Persisted, reorderable display order
- [x] `StackNode.display_order` + `Default` derive + literal fixups
- [x] `Stack::display_order()` with the wholesale topo fallback
- [x] `assign_missing_display_order` wired into every stack write
- [x] `move_planned_pr_node` + `ReorderPlannedPr` RPC + daemon handler
- [x] Web: parse `displayOrder`, `orderStackNodes`, move up/down controls

### Milestone 2: Expandable rows and session navigation
- [x] `PlannedPrRow` three-region restructure; expansion `Set` in `PlannedPrList`
- [x] `parentTitles`, new detail lines
- [x] `onOpenSession` threaded through `WorkflowViewContext`; `boundChildSession`; the wrapped chip

### Milestone 3: Base-sync status
- [x] `tddy_core::base_sync` + real-git tests
- [x] Proto additions; `query_branch` base-sync and dirty legs; `base_sync_cache`
- [x] Web: `branchQueries`, `useQueryBranch` by branch+base, `baseSyncStatus`, four badges

### Milestone 4: Pull from base
- [x] Git primitives + `pull_base_into_node_branch`
- [x] `PullBaseIntoBranch` RPC + daemon handler
- [x] Web: merge/rebase controls, dirty-worktree confirm, in-flight and error states

### Milestone 5: Documentation
- [x] `docs/ft/coder/pr-stack-live-status.md` — § Panel UX, decisions D21–D33
- [x] `docs/ft/web/session-drawer.md` — § Planned-PR list row anatomy, badges, controls
- [x] `docs/ft/coder/changelog.md` and `docs/ft/web/changelog.md`
- [x] `docs/ft/coder/pr-stacking.md` — cross-reference from § Web UI
- [x] `packages/tddy-daemon/docs/connection-service.md` — the two new RPCs and the base-sync leg

**Deferred to the wrap** (`/wrap-context-docs`, not this changeset): the `docs/dev/changesets.md`
index line, the package `changesets.md` entries, and moving the PRD out of `1-WIP`. That index
records *wrapped* changesets — every existing entry ends with "WIP sources … removed after wrap" —
so writing one now would claim a wrap that has not happened.

## Testing Strategy

### Acceptance Tests

Numbered against the PRD's acceptance criteria.

**Cypress** (`packages/tddy-web/cypress/component/`), mounting the whole `SessionsDrawerScreen` over
`anInMemoryRpcBackend`, selectors only in `prStackScreenPage`:

- [x] `PrStackRowExpansionAcceptance.cy.tsx` — AC 1-5
- [x] `PrStackSessionNavigationAcceptance.cy.tsx` — AC 6-7
- [x] `PrStackPlannedPrListAcceptance.cy.tsx` (extend) — AC 8-10; the existing topological-order test stays
      unchanged and becomes the legacy-fallback case
- [x] `PrStackReorderAcceptance.cy.tsx` — AC 11
- [x] `PrStackBaseSyncStatusAcceptance.cy.tsx` — AC 12-15
- [x] `PrStackSyncFromBaseAcceptance.cy.tsx` — AC 16-22

**Rust**:

- [x] `packages/tddy-core/tests/branch_base_sync_acceptance.rs` — AC 23 (the load-bearing
      "leaves the worktree, index and HEAD untouched" test, asserted across a *conflicting* probe), plus
      counts, conflicts, remote-vs-local base resolution, and unavailable-not-clean for every failure mode
- [x] `packages/tddy-workflow-recipes/tests/pr_stack_display_order_acceptance.rs` — append at bottom, no
      renumbering above, delete leaves survivors alone, legacy numbered on next write, reseed follows the new
      plan, move up/down swaps one pair and no-ops at the ends
- [x] `packages/tddy-workflow-recipes/tests/pr_stack_pull_base_acceptance.rs` — merge pushes;
      already-up-to-date does nothing; a conflicting merge leaves the worktree exactly as it was; a dirty
      worktree is refused *before* the fetch with the edit byte-identical; untracked files do not block;
      commit-and-push-then-pull lands both; rebase force-pushes with a lease; a remote that moved reports the
      failed push rather than overwriting
- [x] `packages/tddy-daemon/tests/query_branch_resolution_acceptance.rs` (extend) — AC 24
- [x] `packages/tddy-daemon/tests/base_sync_cache_unit.rs` — no re-probe for unchanged commits, re-probe when
      either moves, failures remembered, capped

**Web unit** (`bun:test`, beside sources): `stackPlan`, `orderStackNodes` (including "a row keeps its position
when its parents change under it"), `parentTitles`, `boundChildSession`, `branchQueries`, `baseSyncStatus`
(each precedence clause, and explicitly that unavailable-with-zero-counts is not in-sync).

**Regression, must stay green unedited:** `PrStackBranchAndPrVisibility`, `PrStackBranchQuery`,
`PrStackLiveStatus`, `PrStackMissingBranch`, `PrStackOrphanedNode`, `PrStackRepointDeadEnd`,
`PrStackStartSessionBaseBranch*`, `PrStackStartSessionModal`, `PrStackAddPlannedPr`, `PrStackPlannedPrPanel`.
This is why the row detail is hidden rather than unmounted — eight of these assert the detail lines with
`exist` / `contain.text`, which pass inside a `display:none` subtree.

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| Ordering rules (assign, append, delete, reseed, move) | Rust integration, real changeset files | The rules are about what lands on disk across `update_stack_atomic`; a unit test on a `Stack` value would not catch a mutator that forgets to call the normalizer |
| `display_order()` fallback and cycle behaviour | Rust unit, in `changeset.rs` | Pure function of a `Stack` value |
| Base-sync probe | Rust integration, real temp git repos | Its whole contract is about real git behaviour and real side effects; a fake would assert our own assumptions back at us |
| Cache behaviour | Rust unit, closure counter | Isolates "did we re-probe" from git itself |
| `query_branch` leg composition | Rust integration | The non-erroring contract is a property of the handler, not of the probe |
| Pull operations | Rust integration, real repos with a bare origin and a linked worktree | Force-push leases, abort-on-conflict and refuse-when-dirty are only meaningful against real git |
| Row layout, badges, controls, navigation | Cypress component over the in-memory RPC backend | The house idiom; exercises the real component tree and the real RPC shapes |
| Pure view logic (order, bound session, badge precedence, query set) | `bun:test` beside the source | Fast, exhaustive over cases that would be tedious to set up through a mount |

## Technical Debt

- ~60 exhaustive `StackNode` literals get `display_order: None` in this changeset. Adding `Default` to the
  derive list makes the *next* additive field a one-site change; converting the existing literals to
  `..StackNode::default()` is deliberately not attempted here.
- `orchestrate_pr_stack/git_ops.rs::rebase_onto` runs `git checkout` in the repo root — a latent clobbering
  hazard. This changeset does not extend it to a new caller (the pull path operates in the node's own
  worktree) but does not fix it either.
- Four unshared private `run_git` helpers remain across the workspace; `base_sync.rs` adds a fifth rather than
  unifying them, which is a larger refactor than this changeset should carry.
- Default-branch resolution stays duplicated between `tddy_core::resolve_default_integration_base_ref` and
  `tddy-tools`' own `symbolic-ref` call.
