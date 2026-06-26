# PR stacking

**Product area:** Coder  
**Updated:** 2026-06-26

## Summary

PR stacking support adds a **parent orchestrating session** — with its own worktree and branch (usually the final PR in the stack) — that coordinates a stack of child PR sessions. When concurrent PRs exist the stack is a **DAG (directed acyclic graph)** rather than a simple chain. Two new workflow recipes handle the two phases: `plan-pr-stack` produces the machine- and human-readable stack plan, and `orchestrate-pr-stack` drives the resumable merge-and-repoint loop until all PRs land on the main branch.

The design extends the existing single-level session chaining mechanism (see [Git integration base ref — Session chaining](git-integration-base-ref.md)) to a full ordered graph, closes the Telegram-only gap by exposing transport-agnostic spawn helpers and CLI flags, and adds a uniform progress-tracking contract that all child sessions satisfy via host-level hooks rather than agent promises.

## Session YAML enrichment

Two new optional fields are added to `Changeset` in `tddy-core`. Both are additive (`Option` + `serde default`) and are forward/backward safe because `Changeset` carries no `deny_unknown_fields` guard.

- **`stack: Option<Stack>`** — present only on the **orchestrator** session. Holds the full PR DAG. Child sessions never carry this field.
- **`orchestrator_session_id: Option<String>`** — present only on **child** sessions. A back-reference from child to its orchestrating session. This is distinct from `SessionMetadata.previous_session_id`, which points at the base-branch source (in a DAG that may be a sibling node, not the orchestrator). Having two separate links allows a child's git history to be built on a sibling while its orchestrator is still traceable.

## Stack data model

The orchestrator's `Changeset.stack` field holds a `Stack` value:

```
Stack { version: u32, nodes: Vec<StackNode> }
```

Each node in the DAG is a `StackNode`:

| Field | Description |
|-------|-------------|
| `node_id` | Stable planner identifier (e.g. `"n1"`). Exists before a child session is materialized. |
| `title` | Human-readable PR title. |
| `description` | Longer description for the PR body. |
| `branch_suggestion` | Planner's suggested branch name; absent until materialised. |
| `branch` | Actual branch name once the child worktree exists. |
| `session_id` | Child session id once the node is materialised. |
| `parents` | List of parent `node_id` values. Empty list = root node (branches off the stack base). More than one entry = DAG node that integrates multiple unmerged parents. |
| `pr_status` | Mirrors `GithubPrStatus` (`phase` one of `planned`, `open`, `merged`, `closed`, `error`). |
| `child_state` | Coarse mirror of the child session's `WorkflowState`. |

**Derived, never persisted:** effective base refs are computed on demand, not stored. The predicate `StackNode::is_skipped()` returns true when `pr_status.phase == "merged"`. Base-ref derivation climbs the `parents` list, skipping merged ancestors, and returns the nearest non-skipped ancestor branches as `origin/<branch>` refs; when all ancestors are merged the node's effective base collapses to the stack bottom (i.e. `origin/main` or equivalent).

**Helpers on `Stack`:** `topo_order` (Kahn sort; cycle → error), `effective_base_refs(node_id, stack_bottom_base) -> Vec<String>`, `node(node_id)`.

**Read/write helpers** (all use temp-then-rename atomicity via `write_changeset_atomic`):

- `update_stack_atomic(orchestrator_dir, f)` — apply a mutation closure to the stack and persist.
- `link_stack_node_to_child_session(orchestrator_dir, node_id, child_session_id, branch)` — record session id and branch on the node after spawning.
- `sync_stack_node_from_child(orchestrator_dir, sessions_root, node_id)` — read the child's `changeset.yaml` and propagate `state.current` → `child_state` and `workflow.github_pr_status` → `pr_status`.

## Child linking

### CLI flags

Two new flags are added to `tddy-coder` (`run.rs`):

- `--stack-parent <orchestrator-session-id>` — sets `Changeset.orchestrator_session_id` on the child, recording which session owns the stack.
- `--stack-base <base-session-id>` — sets `SessionMetadata.previous_session_id` and derives the integration base ref, then calls `spawn_chain_child_worktree`. Defaults to the orchestrator session when omitted (linear stack case where the orchestrator's branch is also the base).

### Transport-agnostic spawn helper

`spawn_chain_child_worktree(sessions_root, parent_session_id, child_session_dir, child_repo, explicit_base)` lives in `packages/tddy-core/src/session_chain.rs`. It lifts the resolve-and-integrate logic that was previously embedded in `telegram_session_control.rs::merge_chain_integration_base_with_explicit_operator_overrides`, which becomes a thin wrapper. The orchestrator recipe calls this helper with the node's derived effective base ref, which may point at a sibling session's branch rather than the orchestrator's own branch.

### Two distinct links per child

Every child session carries:

1. `SessionMetadata.previous_session_id` (+ `Changeset.worktree_integration_base_ref`) — the **base-branch source**; in a DAG this is the sibling node whose branch the child builds on.
2. `Changeset.orchestrator_session_id` — the **coordinating orchestrator**; always the orchestrator session regardless of which sibling supplied the branch base.

## plan-pr-stack recipe

- **CLI name:** `plan-pr-stack`
- **`uses_primary_session_document`:** `false` (no PRD-style document approval gate).
- **Pipeline:** `analyze-stack` → `write-stack-plan` → `end`
  - `analyze-stack` — read-only, `PermissionHint::ReadOnly`, no structured submit. The agent studies the feature description and plans how to split it into a PR stack or DAG.
  - `write-stack-plan` — the agent emits both artifacts via `tddy-tools submit`. No structured JSON goal schema is shared with TDD; the submit carries the YAML plan payload.
- **Artifacts (manifest):** `stack_plan → stack-plan.yaml` (machine-readable DAG), `stack_plan_md → pr-stack-plan.md` (human narrative). Both are injected into the agent context header on each turn.
- **`stack-plan.yaml` contract:** a versioned list of PR nodes, each with `node_id`, `title`, `description`, `branch_suggestion`, `parents` (list of `node_id` strings; empty for roots), and optional `child_recipe` (defaults to `tdd`). Multiple entries in `parents` express a genuine DAG dependency.
- **Parser types** in `plan_pr_stack/mod.rs`: `StackPlanOutput { version, prs: Vec<PlannedPr> }`, `PlannedPr { node_id, title, description, branch_suggestion, parents, child_recipe }`, and `planned_prs_into_stack_nodes(prs) -> Vec<StackNode>`. Validation: unique `node_id`s, all referenced `parents` resolve, no cycle detected via `Stack::topo_order`.
- **State table:** `Init | AnalyzeStack → analyze-stack`; `WriteStackPlan → write-stack-plan`; `StackPlanned → Done`; `Failed → None`; fallback → `analyze-stack`. `status_for_state`: `StackPlanned → "Completed"`, `Failed → "Failed"`, else `"Active"`.
- **Separation of concerns:** the planner emits `stack-plan.yaml` only. The **orchestrator** materialises sessions from the plan when it starts. This keeps the planning recipe pure (plan-in/plan-out) and independently testable with fixtures.

## orchestrate-pr-stack recipe

- **CLI name:** `orchestrate-pr-stack`
- **Pipeline:** a single recurring goal `assess` that routes to task nodes without agent invocation.
- **Custom tasks:** working nodes (`spawn`, `merge`, `repoint`) are custom `Task` implementations, not backend invocations. No `tddy-tools submit` is used by this recipe.
- **Loop shape (engine-native):**

```
assess --GoTo--> spawn        --GoTo("assess")-->
assess --GoTo--> merge        --GoTo("repoint")--> repoint --GoTo("assess")-->
assess --GoTo--> end (EndTask)
assess (Wait)  --> WaitForInput
```

`FlowRunner` executes one task, persists, and returns — that boundary is also the resumability boundary. The loop is idempotent because `assess` recomputes the full world state from durable inputs on every entry.

### State table

`Init, Planning, Spawning, Building, ReadyToMerge, Merging, Repointing, Done, Failed`. Every non-terminal state maps to the single goal `assess` via `next_goal_for_state`. `status_for_state`: `Done → "Completed"`, `Failed → "Failed"`, else `"Active"`.

### `assess` decision algorithm (priority order)

Each tick the orchestrator reads the parent `Changeset.stack`, each child's `changeset.yaml` (authoritative), `artifacts/stack-progress.json`, and live GitHub state via `GithubPrApi`.

1. Any node failed or PR unexpectedly closed → `MarkFailed` (pause loop, surface error to operator).
2. Any node where all dependencies are merged, the PR is open, and its effective base is already the main branch → `Merge` (subject to operator gate, see below).
3. Any node not yet materialised whose parents are all spawned-or-merged → `Spawn` (bottom-to-top, topological order).
4. Any node still building or PR queued with nothing else actionable → `Wait` (`WaitForInput`).
5. All nodes merged → `Done`.

### Operator merge gate

The context flag `orchestrator_autonomous_merge` (persisted under `Changeset.workflow`, rehydrated by `merge_persisted_workflow_into_context`) controls the gate:

- **`false` (default):** before each merge, `assess` returns `WaitForInput` with a prompt asking the operator to approve merging PR #N on branch X. Operator approval sets a one-shot context key `merge_approved_for_node_<id>` and re-runs.
- **`true`:** `assess` routes directly to the `merge` task without an approval pause.

### GitHub API surface

`GithubPrApi` trait (real implementation + stub for tests): `get_open_pr`, `merge_pr(number)`, `patch_pr_base(number, new_base)`, `create_pr(head, base, title, body)`. Backed by shared curl helpers `curl_github_patch_json` and `curl_github_post_json` in `github_rest_common.rs` (alongside the existing GET and PUT helpers from `merge_pr/github.rs`).

## Progress tracking contract

Each child session is obliged to maintain `artifacts/stack-progress.json`. This obligation is a **host guarantee** (written by a shared child hook's `after_task`, not an agent promise):

```
{ node_id, phase, branch, pr_number, pr_url, updated_at, error }
```

`phase` is one of: `building`, `ready_for_pr`, `pr_open`, `done`, `failed`.

The child hook derives values from the child's own `state.current` and `workflow.github_pr_status`. The file is registered as a manifest artifact and a context-header line informs the child agent that it is operating as node N in a PR stack.

**Orchestrator rollup:** `OrchestratePrStackHooks::after_task` regenerates `parent_dir/artifacts/stack-status.md` and `stack-status.json` after every iteration. The rollup table covers: node, branch, dependencies, child phase, PR number, PR state, effective base, and last action taken.

The orchestrator reads `changeset.yaml` as authoritative and `stack-progress.json` as a recipe-agnostic supplementary signal.

## Merge and repoint

After the operator approves (or autonomous mode is enabled) the orchestrator merges via the GitHub REST API (`GithubPrApi::merge_pr`). For each dependent node of the just-merged node:

1. **GitHub base update:** `patch_pr_base(dependent_pr_number, new_base)` where `new_base` is the recomputed effective base (now `main` or the next non-skipped ancestor).
2. **Git history repoint:** in a dedicated scratch worktree for the dependent branch: `git rebase --onto <new_base> <old_base> <branch>`, then `git push --force-with-lease=<branch>:<expected-sha>`. A `git merge-base` fallback guards against a stale `<old_base>`. `git rerere` is enabled at bootstrap.

Rebase conflicts in v1 are not resolved automatically: the node is marked `Failed`, the loop pauses, and the error (branch + git stderr) is surfaced in `stack-status.md`. Agent-assisted resolution is deferred to a later release.

## Full DAG handling

GitHub PRs have a single base ref, so a node that depends on multiple unmerged parents requires special treatment:

- The orchestrator maintains a local `stack-int/<node_id>` integration ref, produced by merging all non-skipped parent branch tips. The node's branch is created or rebased onto this integration ref.
- The **GitHub PR base** points at the **first** non-skipped parent (the primary spine). Commits from the other parents arrive via the integration ref merge.
- As parents merge to the main branch, `effective_base_refs` shrinks. The integration ref is refreshed when the effective parent set changes.
- A multi-parent node's PR is only offered for merge once **all** its parents are merged (so its effective base collapses to the main branch, matching step 2 of the `assess` algorithm).

## Resumability and crash safety

**Loop resumability** is free: `assess` is idempotent, and every non-terminal state maps to `assess`. Restarting the orchestrator session re-enters `assess` exactly as if the previous tick completed.

**Merge+repoint atomicity:** a transient journal file `parent_dir/.workflow/stack-op.json` (written via temp-then-rename) records the in-flight operation:

```
StackOpJournal { op_id, merged_node_id, merge_phase, pre_op_snapshot, dependents }
```

`merge_phase` transitions: `Planned → PrMerged { sha } → RepointingDependent { idx } → Done`. Each transition is an atomic rename.

A recovery guard at the top of every `assess` entry (`recover_in_flight_stack_op`) checks for an in-flight journal: if the phase is `>= PrMerged` but not `Done`, the orchestrator verifies the merge via GitHub and **resumes repointing** (never re-merges). If the phase is `Planned` and the PR is still open, the operation can be safely retried or aborted. All repoint steps are idempotent: `patch_pr_base` to the correct base is a no-op; `rebase --onto` with the merge-base guard re-runs cleanly; `rerere` replays conflict resolutions.

`--force-with-lease=<branch>:<expected-sha>` ensures that a concurrent child push aborts the repoint and routes to `MarkFailed` rather than silently clobbering work.

## Web UI: session creation and drawer grouping

### `orchestrator_session_id` in proto

`Changeset.orchestrator_session_id` (a child-session back-reference to the orchestrating session) is surfaced to the web via `SessionEntry` proto field 21 (`orchestrator_session_id: string`). It is populated during enrichment by `session_list_enrichment.rs` reading the child's `changeset.yaml` (alongside `changeset.state.current`). Empty string for non-child sessions.

### New-session screen: recipe dropdown and parent picker

`CreateSessionPane` (`packages/tddy-web/src/components/sessions/`) gains two changes for tool sessions:

- **Recipe dropdown** — the free-text recipe input is replaced with a `<select>` listing the canonical recipe set (`tdd`, `tdd-small`, `bugfix`, `free-prompting`, `grill-me`, `plan-pr-stack`, `orchestrate-pr-stack`). The canonical list lives in `packages/tddy-web/src/utils/recipeOptions.ts`. Both PR-stack recipes also need to be added to the `--recipe` `value_parser` in `packages/tddy-coder/src/run.rs` (they were previously omitted).
- **Parent stack picker** — a new optional "Parent orchestrator" `<select>` (tool type only) that lists existing sessions identified as PR-stack orchestrators (sessions that have at least one child referencing them via `orchestratorSessionId`). A helper `stackParentCandidates(sessions)` in `packages/tddy-web/src/utils/stackParents.ts` computes the candidate set. Selecting a parent passes `stack_parent` (proto `StartSessionRequest` field 15) to the daemon, which threads it through `SpawnOptions` → `--stack-parent <id>` CLI arg.

### Session drawer: children collapsed under the main stack session

`SessionDrawer` (`packages/tddy-web/src/components/sessions/SessionDrawer.tsx`) renders PR-stack sessions in a collapsible group rather than a flat list.

Grouping logic lives in `packages/tddy-web/src/utils/sessionStackGroups.ts`:

```
groupSessionsByStack(sessions) → { groups: { parent: SessionEntry, children: SessionEntry[] }[], flat: SessionEntry[] }
```

- **Child** = session with a non-empty `orchestratorSessionId` pointing at a present session.
- **Parent** = session referenced as `orchestratorSessionId` by at least one child.
- Children whose parent is absent fall into `flat` (like `isSessionOrphan` in `sessionProjectTable.ts`).
- Within each group, parent and children are sorted by `sortSessionsByCreation`; groups themselves ordered by parent `createdAt` (newest first).

`SessionDrawer` replaces its flat `sessions.map` with:
- Per group: a native `<details data-testid="sessions-drawer-stack-<parentId>" open>` whose `<summary>` contains the parent's `SessionDrawerItem` and whose body renders children with `depth={1}`.
- Then `flat` sessions with `depth={0}`.

`SessionDrawerItem` gains `depth?: number` (indentation) and a chevron indicator for group parents. The native `<details>`/`<summary>` collapse pattern reuses `ConnectionScreen.tsx:2140-2154`.

Only one nesting level is rendered for v1; the grouping utility is written to support recursion later.

## Related

- [Session drawer](../web/session-drawer.md) — session drawer screen layout, create session, recipe field, grouping.
- [Git integration base ref (worktrees)](git-integration-base-ref.md) — session chaining, `spawn_chain_child_worktree`, worktree base-ref validation.
- [Session layout](session-layout.md) — session directory structure, `changeset.yaml`, artifact paths.
- [Workflow recipes](workflow-recipes.md) — `WorkflowRecipe` trait, recipe resolution, `approval_policy`, shipped recipes.
