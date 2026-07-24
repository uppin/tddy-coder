# PR stacking

**Product area:** Coder  
**Updated:** 2026-07-03

## Summary

PR stacking support adds a **single orchestrating session** — with its own worktree and branch (usually the final PR in the stack) — that plans a stack of child PR sessions and then **operates that stack interactively** with the developer. When concurrent PRs exist the stack is a **DAG (directed acyclic graph)** rather than a simple chain.

> **Updated 2026-07-03 — free-prompting operator loop.** The orchestrator no longer runs an **automatic agentic loop**. Previously, after planning, the `pr-stack` recipe auto-cycled `assess → spawn / merge / repoint` and drove the whole stack to master with no human turn-by-turn. That autopilot is **removed**. Now, once the plan exists, the same orchestrator session drops into an **interactive free-prompting chat** (the `orchestrate` goal): the developer prompts the agent, and the agent manages the stack explicitly through a new set of **PR-management tools** exposed by `tddy-tools` (see [PR-management tools](#pr-management-tools)). Each planned PR also gains an **internal status** — a computed "does this node need action?" signal (e.g. `needs-repoint`, `has-conflicts`, `ready-to-merge`) that is auto-derived from git + GitHub reality but can be overridden/annotated by the agent (see [Internal PR status](#internal-pr-status)). The planning phase (`analyze-stack` → `write-stack-plan`) is unchanged and still automatic; only the *drive* phase becomes operator-driven. The `assess` decision function, merge/repoint bridge, and `GithubPrApi` are retained but are now invoked **on demand by the tools**, not by an autonomous loop.

> **Updated 2026-07-01 — unified `pr-stack` recipe.** The plan phase and the orchestrate phase used to be two separate recipes and two separate sessions (`plan-pr-stack` then a follow-on `orchestrate-pr-stack` session seeded from the first). They are now **one recipe, one session**: `pr-stack` (see [pr-stack recipe](#pr-stack-recipe) below). The legacy CLI names `plan-pr-stack` and `orchestrate-pr-stack` remain accepted as **aliases** that resolve to the same unified recipe — existing scripts, YAML, and `--recipe` invocations keep working. This consolidation is what makes the web UI's [PR-Stack Chat Screen](../web/session-drawer.md#per-workflow-session-views) possible: a single session can show the planned-PR list and let the operator keep refining the plan via chat, without switching sessions to start orchestration.

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
| `pr_status` | Mirrors `GithubPrStatus` (`phase` one of `planned`, `open`, `merged`, `closed`, `error`). Reflects **GitHub reality**. |
| `child_state` | Coarse mirror of the child session's `WorkflowState`. |
| `internal_status` | Optional `PrInternalStatus` — the **action-needed** signal, orthogonal to `pr_status`. See [Internal PR status](#internal-pr-status). |

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

## pr-stack recipe

- **CLI name:** `pr-stack` (canonical). Legacy aliases `plan-pr-stack` and `orchestrate-pr-stack` still resolve to the same recipe (`recipe_resolve.rs`) — see [Legacy aliases](#legacy-aliases) below.
- **`uses_primary_session_document`:** `false` (no PRD-style document approval gate).
- **One session, whole lifecycle:** the same session that analyzes the feature and writes the plan also operates it to master — there is no second "orchestrate" session seeded from the first.
- **Pipeline:** `analyze-stack` → `write-stack-plan` → `orchestrate` (terminal interactive loop)
  - `analyze-stack` — read-only, `PermissionHint::ReadOnly`, no structured submit. The agent studies the feature description and plans how to split it into a PR stack or DAG.
  - `write-stack-plan` — the agent emits both plan artifacts via `tddy-tools submit`. No structured JSON goal schema is shared with TDD; the submit carries the YAML plan payload. Seeding `Changeset.stack` from the written plan happens here / on entry to `orchestrate` (idempotent — same guard as the old `seed_orchestrator_stack_from_plan`).
  - `orchestrate` — the **free-prompting operator loop**. A single `BackendInvokeTask` goal with **no `end` edge**: `FlowRunner` hits "no successor" and pauses as `WaitingForInput`, keeping the session `Running` for multi-turn chat (identical mechanism to the `free-prompting` recipe). `PermissionHint::AcceptEdits` (the agent edits files during conflict resolution), and its allowed tools are the [PR-management tools](#pr-management-tools) plus `Agent`. There is **no** automatic `assess → spawn / merge / repoint` cycle; the developer prompts the agent, which calls the tools explicitly. Each `orchestrate` turn's prompt is preceded by the `<context-reminder>` header (`before_task` → `prepend_context_header`) listing the manifest docs that exist on disk — so the planning knowledge in `artifacts/exploration.md` (and the other stack artifacts) is advertised to the operator agent; when no such file exists no header is injected.
- **Removed:** the `begin-orchestrate` host bridge task and the `assess` / `spawn` / `merge` / `repoint` graph nodes and edges. The underlying helpers (`assemble_views`, `effective_base_ref`, `execute_stack_merge`, `execute_stack_repoint`, `RealGithubPrApi`, conflict detection) are **kept** — they are now called by the PR-management tools rather than by graph tasks.
- **Artifacts (manifest):** union of the plan and orchestrate artifacts — `stack_plan → stack-plan.yaml`, `stack_plan_md → pr-stack-plan.md`, `stack_status_md → stack-status.md`, `stack_status_json → stack-status.json`, and `exploration → exploration.md` (the code-discovery map, see [Exploration artifact](exploration-artifact.md)).
- **`stack-plan.yaml` contract:** a versioned list of PR nodes, each with `node_id`, `title`, `description`, `branch_suggestion`, `parents` (list of `node_id` strings; empty for roots), and optional `child_recipe` (defaults to `tdd`). Multiple entries in `parents` express a genuine DAG dependency. `branch_suggestion` is **required** and must follow the grouped convention `feature/<stack-slug>/<node>` (e.g. `feature/auth/token-store`, `feature/auth/middleware`) — every PR shares one `feature/<stack-slug>/` namespace so the stack's branches group together, and "Start session" always has a concrete branch to create. The submit may also carry an **optional top-level `exploration` string** (a markdown code-discovery map with `path:line` references); when non-blank the host persists it to `artifacts/exploration.md`, matching the tdd/tdd_small/bugfix planning recipes. A blank/absent field writes no file.
- **Parser types** in `plan_pr_stack/mod.rs` (reused as-is by the unified recipe): `StackPlanOutput { version, exploration: Option<String>, prs: Vec<PlannedPr> }`, `PlannedPr { node_id, title, description, branch_suggestion, parents, child_recipe }`, and `planned_prs_into_stack_nodes(prs) -> Vec<StackNode>`. Validation (`validate_stack_plan`): unique `node_id`s, all referenced `parents` resolve, no cycle detected via `Stack::topo_order`, and every `branch_suggestion` is present, in `feature/<stack>/<node>` form, and shares one `feature/<stack>/` namespace.
- **State table:** `Init | AnalyzeStack → analyze-stack`; `WriteStackPlan → write-stack-plan`; `StackPlanned → orchestrate` (drops into the interactive loop — **not** a terminal "Completed" state); `orchestrate → orchestrate` (pauses for input each turn); `failed → None`. `next_goal_for_state_with_changeset` still disambiguates a legacy `"Init"` with a populated `Changeset.stack` by resuming into `orchestrate` (previously `assess`). `status_for_state`: `StackPlanned | orchestrate → "Active"`, `failed → "Failed"`, else `"Active"`.
- **Refining the plan via chat:** `plan_refinement_goal()` returns `write-stack-plan` — the same goal used to author the plan. After the plan exists (state `StackPlanned`), the operator can keep chatting; each refinement turn re-runs `write-stack-plan` on the **same session**, the agent re-emits `stack-plan.yaml`, and the host re-validates and re-seeds `Changeset.stack` (`reseed_stack_from_plan_if_unspawned`) — overwriting `version` + `nodes` wholesale as long as no node has been materialised into a child session yet. Once a node has a `session_id`, further refinement of that node is refused so an in-progress child session is never orphaned. An invalid refinement (cycle, dangling parent) is rejected and the previously-persisted stack is left untouched. On resume/continue, `StackPlanned` moves on into `assess` — refinement is an operator-initiated side path, not the default resume target.

### Loop shape (free-prompting)

```
analyze-stack --GoTo--> write-stack-plan --GoTo--> orchestrate
orchestrate (no successor edge) --> WaitForInput   (each turn)
```

`FlowRunner` executes one task, persists, and returns. `orchestrate` has **no** outgoing edge, so after each backend turn the runner finds no successor and pauses as `WaitingForInput`, keeping the session `Running` — the developer sends the next prompt, the agent responds and (optionally) calls PR-management tools, and the cycle repeats. This is the same pause-for-input mechanism the `free-prompting` recipe relies on; there is no autonomous merge/repoint cycle.

### Legacy aliases

> **Note (2026-07-03):** the `OrchestratePrStackRecipe` struct and its engine-driven `assess → spawn/merge/repoint` graph are **retained but inert** — no CLI name resolves to it (all three resolve to `PrStackRecipe`), so it is never instantiated in production. It is deliberately kept for its acceptance-test coverage of the engine-driven orchestration logic whose helpers (`assemble_views`, `decide_next_action`, `execute_stack_merge`, `execute_stack_repoint`) are reused on demand by the free-prompting `pr_*` tools. This is a documented decision, not an oversight; remove it only together with that test coverage.

`plan-pr-stack` and `orchestrate-pr-stack` remain in `approval_policy::supported_workflow_recipe_cli_names()` and both resolve, via `recipe_resolve.rs`, to the same `PrStackRecipe` (i.e. `recipe.name() == "pr-stack"` regardless of which of the three CLI names was used to start the session). A legacy on-disk session created before the consolidation (recipe field still `"plan-pr-stack"` or `"orchestrate-pr-stack"`, state possibly the old orchestrate-only `"Init"` — which never advanced during that recipe's healthy operation) resumes correctly because `PrStackRecipe` overrides a new `WorkflowRecipe::next_goal_for_state_with_changeset` trait method (default: delegates to `next_goal_for_state`, ignoring the changeset) to disambiguate `"Init"` using `Changeset.stack`: a populated stack means orchestration is already under way, so resume goes to `assess`; an empty/absent stack means a genuinely fresh session, so resume goes to `analyze-stack`. `start_goal_for_session_continue` (`tddy-core/src/changeset.rs`) calls the changeset-aware method — the bare `next_goal_for_state` alone cannot make this distinction, since it never sees the changeset.

## PR-management tools

During the `orchestrate` goal the agent has a set of `tddy-tools` MCP tools (names `mcp__tddy-tools__pr_*`) that let it manage the stack explicitly. They are added to the MCP tool router in `tddy-tools` (`server.rs`), advertised automatically via the session's MCP config, and auto-allowed by the permission `decide()` (all `mcp__tddy-tools__*` are allowed without a prompt). Each tool operates on the **orchestrator session's changeset** (located via the existing session-context plumbing; read/write via `changeset::{read_changeset, update_stack_atomic}`) and, where relevant, live GitHub + git.

| Tool | Purpose | Reuses |
|------|---------|--------|
| `pr_stack_status` | List every node with its live GitHub state (`PrLiveStatus`) and its computed [internal status](#internal-pr-status); writes derived statuses back to the changeset. | `assemble_views`, `effective_base_ref`, `PrLiveStatus` |
| `pr_merge` | Merge a node's PR into its base. | `RealGithubPrApi::merge_pr` / `execute_stack_merge` |
| `pr_repoint` | Repoint a node's PR base branch after an ancestor merges. | `RealGithubPrApi::patch_pr_base` / `execute_stack_repoint`, `effective_base_ref` |
| `pr_close` | Close a PR without merging. | new `close_pr` helper (`PATCH /pulls/{n}` `{state: "closed"}`) |
| `pr_resolve_conflicts` | Sync a node's branch with its base, detect conflicts (`git ls-files -u`), and return the conflicted paths so the agent resolves them in the worktree; marks the node `has-conflicts`. | `merge_pr/git_ops.rs::sync_feature_with_origin_main`, `ensure_no_unmerged_paths` |
| `pr_set_status` | Agent override: set a node's internal status `kind` + `note` with `source = "override"`. | `update_stack_atomic` |
| `pr_add_planned` | Add/amend a planned PR node mid-flow. | `pr_stack::add_planned_pr_node` |
| `pr_spawn_child` | Start a child coding session for a node (with `stack_parent` set) — the same effect as the web "Start session" CTA, driven from chat. | `StartSession` daemon path (via the toolcall relay) |

Merging and repointing keep their prior crash-safety semantics (`StackOpJournal`, idempotent repoint, `--force-with-lease`), only now they are entered when the agent calls `pr_merge` / `pr_repoint`, not by the loop.

### GitHub API surface

`GithubPrApi` trait (real implementation + mock transport for tests): `get_open_pr`, `merge_pr(number)`, `patch_pr_base(number, new_base)`, `create_pr(head, base, title, body)`, and the new `close_pr(number)`. Backed by shared curl helpers `curl_github_patch_json`, `curl_github_post_json`, and `curl_github_put_json` in `github_rest_common.rs`.

## Internal PR status

`internal_status: Option<PrInternalStatus>` on `StackNode` is the **action-needed** signal, orthogonal to `pr_status` (which mirrors GitHub reality):

```
PrInternalStatus { kind: String, note: Option<String>, source: String }
```

- **`kind`** — one of `up-to-date`, `needs-repoint`, `has-conflicts`, `ready-to-merge`, `blocked`, `merged`.
- **`note`** — optional free-text annotation (agent context, e.g. "waiting on API design").
- **`source`** — `derived` (auto-computed) or `override` (agent-set).

**Derivation** (in `pr_stack_status`, from `NodeView` / `PrLiveStatus` / `effective_base_ref`):

1. PR merged → `merged`.
2. A parent has merged but the node's PR base ≠ its effective base → `needs-repoint`.
3. Syncing the branch with its base surfaces unmerged paths → `has-conflicts`.
4. PR open, all deps merged, no conflicts → `ready-to-merge`.
5. Otherwise → `up-to-date`.

**Override wins:** a node whose `source == "override"` is **not** overwritten by derivation — the agent's manual status (e.g. `blocked` with a note) persists until it clears the override. This is auto-derived + agent override, per the design decision.

`internal_status` is additive (`#[serde(default, skip_serializing_if = "Option::is_none")]`) so old `changeset.yaml` files deserialize with `None`. It rides to the web inside `SessionEntry.stack_plan_json` (no proto change) and renders as a colored badge on each planned-PR row (§ Web UI).

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

Rebase/merge conflicts are surfaced to the agent via `pr_resolve_conflicts`: the tool syncs the branch, detects unmerged paths, marks the node `has-conflicts`, and returns the conflicted file list. The agent then resolves the conflicts directly in the node's worktree (the `orchestrate` goal runs with `AcceptEdits`) and re-runs the tool to confirm a clean tree — replacing the old "mark Failed and pause" behavior.

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

### Context docs in proto

The session's **context documents** — the recipe manifest's planning artifacts (`exploration.md`, `stack-plan.yaml`, `pr-stack-plan.md`, the two `stack-status.*` files) — are surfaced to the web via `SessionEntry` proto field 27, `repeated SessionContextDoc context_docs`, where `SessionContextDoc { key, basename, path, description, exists }`. The list is derived from the recipe manifest and populated during enrichment (`session_list_enrichment.rs`, alongside `orchestrator_session_id`):

- `session_context_docs::context_docs_for_session(recipe_name, session_dir)` joins the manifest's `known_artifacts()` with a per-key human `description` (a new defaulted `SessionArtifactManifest::artifact_doc_descriptions()`; `PrStackRecipe` provides a one-liner per artifact) and an on-disk existence flag, resolving each `path` under `session_artifacts_root` (`session_dir/artifacts/`). A blank or unknown recipe yields an empty list.
- `session_context_docs::read_session_context_doc_utf8(recipe_name, session_dir, basename)` reads a doc's contents, allowlisted to the manifest's basenames with a canonicalize-and-contain guard rooted at the artifacts dir (a non-manifest basename or a traversal segment → `PermissionDenied`). It mirrors the guard shape in `session_workflow_files.rs`. The wire RPC that exposes this reader is added by the web-facing follow-up that consumes it (the **Docs** tab and child "Start session" prompt references).

### New-session screen: recipe dropdown and parent picker

`CreateSessionPane` (`packages/tddy-web/src/components/sessions/`) gains two changes for tool sessions:

- **Recipe dropdown** — the free-text recipe input is replaced with a `<select>` listing the canonical recipe set, including the unified **`pr-stack`** (the legacy `plan-pr-stack` / `orchestrate-pr-stack` entries are no longer offered in the dropdown — new sessions always start as `pr-stack` — though both CLI names still resolve if typed directly). `pr-stack` is included in the `--recipe` `value_parser` in `packages/tddy-coder/src/run.rs` alongside the two legacy aliases.
- **Parent stack picker** — a new optional "Parent orchestrator" `<select>` (tool type only) that lists existing sessions identified as PR-stack orchestrators (sessions whose `recipe` is `pr-stack` — or a legacy alias — and that are not themselves children of another orchestrator; see `prStackOrchestrators()` in `packages/tddy-web/src/utils/stackParents.ts`). Selecting a parent passes `stack_parent` (proto `StartSessionRequest` field 15) to the daemon, which threads it through `SpawnOptions` → `--stack-parent <id>` CLI arg.

### Per-workflow session views: the PR-Stack Chat Screen

Once a `pr-stack` session is selected in the session drawer, the main pane opens a dedicated **PR-Stack Chat Screen** instead of the terminal — a chat window backed by a remote Presenter (over the existing `TddyRemote.Stream` RPC) alongside a live list of the planned PRs with a **"Start session"** CTA per unspawned node. This chat *is* the `orchestrate` free-prompting loop: the developer types instructions ("merge n1", "repoint the dependents", "what needs action?") and the agent responds and calls the [PR-management tools](#pr-management-tools). Full UI spec: [Session drawer § Per-Workflow Session Views](../web/session-drawer.md#per-workflow-session-views).

Each planned-PR row (`PlannedPrRow.tsx`) renders an **internal-status badge** next to the existing phase chip, colored by `internal_status.kind` (e.g. amber `needs-repoint`, red `has-conflicts`, green `ready-to-merge`), with `internal_status.note` as hover text. The badge is parsed from the `internal_status` field carried inside `SessionEntry.stack_plan_json` (`stackPlan.ts::parseStackPlan`).

### Manually adding a planned PR

Until now the only way to change an orchestrator's planned-PR list was **chat-driven refinement** (§ pr-stack recipe, above): the operator asks the agent to re-plan, the agent re-emits `stack-plan.yaml`, and the host overwrites the whole node list (`reseed_stack_from_plan_if_unspawned`) — a round trip through the LLM, and an all-or-nothing rewrite that's refused once any node has a spawned child session.

The PR-Stack Chat Screen's planned-PR list gains a **direct, deterministic path** that doesn't touch the LLM: a "New planned PR" form lets the operator manually add a single node — title, description, optional branch suggestion, optional child recipe, and a **multi-select ancestor picker** listing the orchestrator's existing planned-PR nodes (its chosen ancestors become the new node's `parents`, i.e. `StackNode.parents` — see [Stack data model](#stack-data-model)).

- **RPC:** `ConnectionService.AddPlannedPr(AddPlannedPrRequest) -> AddPlannedPrResponse` (`connection.proto`). Request carries `session_id` (the orchestrator), `title`, `description`, `branch_suggestion`, `parents` (chosen ancestor node ids), and `child_recipe`. Response carries `stack_plan_json` — the same wire shape as `SessionEntry.stack_plan_json` (field 23) — so the web reuses the existing `parseStackPlan` parser rather than a second message schema.
- **Semantics:** appends exactly one `StackNode` (`session_id: None`, `pr_status: None` — stays planned, does not spawn a session) to `Changeset.stack` via `update_stack_atomic`. Unlike chat refinement, this **never touches existing nodes** and is not gated on whether other nodes have already spawned — it's additive only. The node id is server-assigned (never client-supplied). Rejects (without writing) when a chosen ancestor doesn't resolve to an existing node id, or when appending would introduce a cycle (checked via `Stack::topo_order`, same guard as the plan-time validator).
- **UI:** a "+ New planned PR" entry point on the planned-PR list opens the form; ancestors are chosen via checkboxes over the currently-listed nodes (topo order, § Per-workflow session views). On success the list re-renders with the new node included.

Implementation: `tddy_workflow_recipes::pr_stack::add_planned_pr_node` (pure function — read/validate/append/write); daemon handler `ConnectionServiceImpl::add_planned_pr` in `connection_service.rs`.

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

- [Session drawer](../web/session-drawer.md) — session drawer screen layout, create session, recipe field, grouping, and the [Per-Workflow Session Views](../web/session-drawer.md#per-workflow-session-views) / [PR-Stack Chat Screen](../web/session-drawer.md#pr-stack-chat-screen) sections.
- [Git integration base ref (worktrees)](git-integration-base-ref.md) — session chaining, `spawn_chain_child_worktree`, worktree base-ref validation.
- [Session layout](session-layout.md) — session directory structure, `changeset.yaml`, artifact paths.
- [Workflow recipes](workflow-recipes.md) — `WorkflowRecipe` trait, recipe resolution, `approval_policy`, shipped recipes.
