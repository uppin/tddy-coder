# PR stacking

**Product area:** Coder  
**Updated:** 2026-07-30

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
| `branch_suggestion` | Planner's suggested branch name — a *planned* name, set at planning time. It names no ref and never satisfies the spawn gate; it is the name the child worktree is asked to create. |
| `branch` | Actual branch name once the child worktree has created it — "a branch that exists". Planning leaves this `None`. **This is the spawn gate and the durable link key.** |
| `session_id` | Child session id once the node is materialised. A *fallback* route to resolving `branch` only; the stack does not depend on it, and a closed or cleaned-up session never blocks the nodes below. A deleted session leaves the id in place — the node is then **orphaned** and offers "Start session" again (see [PR-Stack live status § Orphaned-node recovery](pr-stack-live-status.md#orphaned-node-recovery-added-2026-07-26)). |
| `parents` | List of parent `node_id` values. Empty list = root node (branches off the stack base). More than one entry = DAG node that integrates multiple unmerged parents. |
| `pr_status` | Mirrors `GithubPrStatus` (`phase` one of `planned`, `open`, `merged`, `closed`, `error`). Reflects **GitHub reality**. |
| `child_state` | Coarse mirror of the child session's `WorkflowState`. |
| `internal_status` | Optional `PrInternalStatus` — the **action-needed** signal, orthogonal to `pr_status`. See [Internal PR status](#internal-pr-status). |

**Derived, never persisted:** effective base refs are computed on demand, not stored. The predicate `StackNode::is_skipped()` returns true when `pr_status.phase == "merged"`. Base-ref derivation climbs the `parents` list, skipping merged ancestors, and returns the nearest non-skipped ancestor **branches** as `origin/<branch>` refs; when all ancestors are merged the node's effective base collapses to the stack bottom (i.e. `origin/main` or equivalent). A non-merged ancestor that owns no branch contributes nothing — it is never given a synthesized `origin/<node_id>` ref, because nothing ever created that ref.

**Helpers on `Stack`:** `topo_order` (Kahn sort; cycle → error), `effective_base_refs(node_id, stack_bottom_base) -> Vec<String>`, `base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>` (the spawn base; refuses when a non-merged parent owns no branch), `node(node_id)`.

### Effective spawn branch *(added 2026-07-26)*

Both the node link and chain-base resolution used to key on the spawn request's `new_branch_name`, which
is **empty** when the operator resumes an existing branch (`work_on_selected_branch`). A resumed spawn
therefore never re-linked to its node — `pr_stack_node_for_spawn` returns `None` for a blank branch — so
each click produced another unlinked session and the row stayed stuck in its recovered state.

The linking key is now the spawn's **effective branch** (`connection_service::effective_spawn_branch`):

- `new_branch_name` (trimmed) when a branch is being created;
- otherwise `selected_branch_to_work_on`, normalised through `tddy_core::worktree::local_branch_name`
  so an `origin/`-prefixed selection resolves to the local branch name;
- a new-branch spawn ignores `selected_branch_to_work_on` entirely — the dialog sends that field
  unconditionally, so "prefer whichever field is non-empty" would pick the wrong one.

Applied at the **node-link** sites in both spawn paths that take a `stack_parent`
(`spawn_claude_cli_session_inner`, `start_sandboxed_claude_cli_session`), so **a resumed spawn re-links
its node** and the recovery sticks. `resolve_chain_base_ref` deliberately keeps keying on
`new_branch_name`: under `work_on_selected_branch` a resolved chain base would make worktree setup run a
real `fetch_chain_pr_integration_base` — a fetch that can fail, and that is pointless for a resume which
creates no branch. `start_sandboxed_cursor_cli_session` takes no `stack_parent` at all and is unaffected
(tracked in `docs/dev/TODO.md`).

**Branch resolution helpers** (free functions in `changeset.rs`): `resolve_stack_node_branch(sessions_root, node) -> Option<String>` — the node's own `branch`, else the `branch` in its child session's changeset (a missing session directory resolves to `None`, never an error) — and `read_stack_with_resolved_branches(sessions_root, orchestrator_session_id)`, the orchestrator's stack with every node's `branch` hydrated through that resolver. The hydrated copy is read-only; persisting it would write a fallback-derived branch onto a node that never recorded one.

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
  - `analyze-stack` — read-only, `PermissionHint::ReadOnly`, no structured submit. The agent studies the feature description and plans how to split it into a PR stack or DAG, subject to the [PR boundary contract](#pr-boundary-contract-every-node-is-self-contained).
  - `write-stack-plan` — the agent emits both plan artifacts via `tddy-tools submit`. No structured JSON goal schema is shared with TDD; the submit carries the YAML plan payload. Seeding `Changeset.stack` from the written plan happens here / on entry to `orchestrate` (idempotent — same guard as the old `seed_orchestrator_stack_from_plan`).
  - `orchestrate` — the **free-prompting operator loop**. A single `BackendInvokeTask` goal with **no `end` edge**: `FlowRunner` hits "no successor" and pauses as `WaitingForInput`, keeping the session `Running` for multi-turn chat (identical mechanism to the `free-prompting` recipe). `PermissionHint::AcceptEdits` (the agent edits files during conflict resolution), and its allowed tools are the [PR-management tools](#pr-management-tools) plus `Agent`. There is **no** automatic `assess → spawn / merge / repoint` cycle; the developer prompts the agent, which calls the tools explicitly. Each `orchestrate` turn's prompt is preceded by the `<context-reminder>` header (`before_task` → `prepend_context_header`) listing the manifest docs that exist on disk — so the planning knowledge in `artifacts/exploration.md` (and the other stack artifacts) is advertised to the operator agent; when no such file exists no header is injected.
- **Removed:** the `begin-orchestrate` host bridge task and the `assess` / `spawn` / `merge` / `repoint` graph nodes and edges. The underlying helpers (`assemble_views`, `effective_base_ref`, `execute_stack_merge`, `execute_stack_repoint`, `RealGithubPrApi`, conflict detection) are **kept** — they are now called by the PR-management tools rather than by graph tasks.
- **Artifacts (manifest):** union of the plan and orchestrate artifacts — `stack_plan → stack-plan.yaml`, `stack_plan_md → pr-stack-plan.md`, `stack_status_md → stack-status.md`, `stack_status_json → stack-status.json`, and `exploration → exploration.md` (the code-discovery map, see [Exploration artifact](exploration-artifact.md)).
- **`stack-plan.yaml` contract:** a versioned list of PR nodes, each with `node_id`, `title`, `description`, `branch_suggestion`, `parents` (list of `node_id` strings; empty for roots), and optional `child_recipe` (defaults to `tdd`). Multiple entries in `parents` express a genuine DAG dependency. `branch_suggestion` is **required** and must follow the grouped convention `feature/<stack-slug>/<node>` (e.g. `feature/auth/token-store`, `feature/auth/middleware`) — every PR shares one `feature/<stack-slug>/` namespace so the stack's branches group together, and "Start session" always has a concrete branch to create. The submit may also carry an **optional top-level `exploration` string** (a markdown code-discovery map with `path:line` references); when non-blank the host persists it to `artifacts/exploration.md`, matching the tdd/tdd_small/bugfix planning recipes. A blank/absent field writes no file.
- **Parser types** in `plan_pr_stack/mod.rs` (reused as-is by the unified recipe): `StackPlanOutput { version, exploration: Option<String>, prs: Vec<PlannedPr> }`, `PlannedPr { node_id, title, description, branch_suggestion, parents, child_recipe }`, and `planned_prs_into_stack_nodes(prs) -> Vec<StackNode>`. Validation (`validate_stack_plan`): unique `node_id`s, all referenced `parents` resolve, no cycle detected via `Stack::topo_order`, and every `branch_suggestion` is present, in `feature/<stack>/<node>` form, and shares one `feature/<stack>/` namespace.
- **State table:** `Init | AnalyzeStack → analyze-stack`; `WriteStackPlan → write-stack-plan`; `StackPlanned → orchestrate` (drops into the interactive loop — **not** a terminal "Completed" state); `orchestrate → orchestrate` (pauses for input each turn); `failed → None`. `next_goal_for_state_with_changeset` still disambiguates a legacy `"Init"` with a populated `Changeset.stack` by resuming into `orchestrate` (previously `assess`). `status_for_state`: `StackPlanned | orchestrate → "Active"`, `failed → "Failed"`, else `"Active"`.
- **Refining the plan via chat:** `plan_refinement_goal()` returns `write-stack-plan` — the same goal used to author the plan. After the plan exists (state `StackPlanned`), the operator can keep chatting; each refinement turn re-runs `write-stack-plan` on the **same session**, the agent re-emits `stack-plan.yaml`, and the host re-validates and re-seeds `Changeset.stack` (`reseed_stack_from_plan_if_unspawned`) — overwriting `version` + `nodes` wholesale as long as no node has been materialised yet. Once a node owns a **`branch` or** a `session_id`, further refinement is refused: the branch is real work that outlives the session that created it, so overwriting the node would orphan the branch as well as any in-progress child session. An invalid refinement (cycle, dangling parent) is rejected and the previously-persisted stack is left untouched. On resume/continue, `StackPlanned` moves on into `assess` — refinement is an operator-initiated side path, not the default resume target.

### PR boundary contract: every node is self-contained

A planned PR must be **independently reviewable and independently mergeable**: the API/schema change, the code implementing it, and its tests land in **one** node. A reviewer can judge it without waiting for a later node in the stack.

**Splitting by layer is forbidden.** These pairs are one node, never two:

| ✗ Layer split (invalid) | ✓ One self-contained node |
|---|---|
| `n1` add proto RPCs → `n2` implement them | `n1` attachment staging: proto + daemon handler + tests |
| `n1` add an endpoint → `n2` add its handler | `n1` the endpoint, serving real responses |
| `n1` add a data model → `n2` persist it | `n1` the model with its persistence |
| `n1` change a signature → `n2` fill in the body | `n1` the working function |

A node that ships only **surface** — RPCs returning `unimplemented`, a field nothing reads, a trait with stub impls — is not a valid PR. It cannot be reviewed for correctness (there is no behavior to check), it cannot be tested beyond compiling, and it leaves a contract in the tree that misrepresents what the system does.

**When a vertical slice is too large, split by capability, not by layer.** Cut along user-visible increments where each part is still end-to-end: one source variant rather than all of them, one scope/enum case rather than the full set, one screen or entry point, the happy path before the edge cases. Each such PR carries its own contract *plus* behavior *plus* tests, and the next node extends it.

**Two narrow exceptions** — a node may omit implementation when it is a purely mechanical rename/move/extraction with no behavior change, or a regeneration of already-committed generated code exposing no new surface. Anything else goes in the node's `description` for a human to decide; the agent is told not to invent a third exception.

This contract is **advisory, not machine-enforced.** It is carried by the `analyze-stack` and `write-stack-plan` system prompts (`pr_stack/hooks.rs`), and appears in **both** deliberately: `write-stack-plan` is the goal `plan_refinement_goal()` returns, so it re-runs on every chat-driven refinement — a rule present only in `analyze-stack` would be silently dropped the first time an operator refined the plan. That copy also tells the agent a refinement request must not talk it into a layer-split stack. Pinned by `pr_boundary_scoping_rule_tests` in `hooks.rs`, which drives the real `before_task` seam rather than asserting on the string constants.

**Why there is no validator for it.** `validate_stack_plan` sees only `node_id`, `title`, `description`, `branch_suggestion`, and `parents` — never the diff a node will eventually produce — so it cannot distinguish a vertical slice from a layer split. Any check reduces to a keyword heuristic over `description` ("reject plans mentioning *proto only*"), which is trivially reworded around and would reject legitimate plans. Enforcement was considered and rejected on those grounds; the guidance is prompt-carried instead, and the node `description` is the escape hatch that surfaces a debatable boundary to a human reviewer. Being guidance to a model, it shapes planning without guaranteeing it: if layer-split stacks keep appearing in practice, the next step is a **plan-review gate** before `orchestrate`, not a regex in the validator.

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
| `pr_add_planned` | Append a planned PR node mid-flow. Additive only — it never touches an existing node; editing one is `pr_update_planned`. | `pr_stack::add_planned_pr_node` |
| `pr_spawn_child` | Start a child coding session for a node (with `stack_parent` set) — the same effect as the web "Start session" CTA, driven from chat. | `StartSession` daemon path (via the toolcall relay) |
| `pr_update_planned` | Edit a node's `title` / `description` (any time, including once it owns a branch, a session and an open PR) and its `branch_suggestion` (only while it owns no branch). Opt-in `sync_pr` also publishes the new title/body to the node's PR. | `pr_stack::{update_planned_pr_node, sync_node_to_github_pr}` |
| `pr_delete_planned` | Remove a node, reparenting its children onto that node's parents. Refuses a node whose PR is open. Branch, worktree and child session are left alone and reported back as unowned. | `pr_stack::delete_planned_pr_node` |
| `pr_set_parents` | Give a node a whole new parent list — the plan-level move, and the only reorder primitive (stack order is derived from `parents`). When the node owns a branch it is realigned exactly as a repoint realigns it. | `pr_stack::set_stack_node_parents` |
| `pr_read` | One PR in full: title, body, state, base/head, mergeability, one latest review state per reviewer, and the head commit's check runs. Changed files only on request. | `pr_insight::read_pr` |
| `pr_search` | Find PRs in this repository — including ones the stack does not track — by text, state, author or base. | `pr_insight::search_repository_prs` |
| `pr_comments` | A PR's review feedback: submitted reviews, diff-anchored comment threads, and conversation comments. | `pr_insight::read_pr_comments` |
| `pr_adopt` | Create a node from an existing PR, bound to its head branch and PR reference. | `pr_stack::adopt_pr_into_stack` |

Merging and repointing keep their prior crash-safety semantics (`StackOpJournal`, idempotent repoint, `--force-with-lease`), only now they are entered when the agent calls `pr_merge` / `pr_repoint`, not by the loop.

### Full control over the plan *(added 2026-07-30)*

The first eight tools could only **grow** the stack. Combined with `reseed_stack_from_plan_if_unspawned` refusing once any node owns a `branch` or a `session_id`, the plan became immutable the moment the stack became real: no way to edit a node, delete one, move one in the DAG, read a PR in any depth, search the repository's PRs, or bring an existing PR in. The seven tools added above close that.

Rules that are not obvious from the table:

- **`pr_repoint` and `pr_set_parents` are different operations.** Repointing answers *"the base branch drifted — retain the parent that owns this target"*; setting parents answers *"the plan changed — this node belongs here now"*, with the caller naming the complete new set (an empty list makes the node a root). They share one git+GitHub tail, `realign_node_to_effective_base`: rebase onto the new effective base, `--force-with-lease` push, re-target the open PR.
- **Deleting reparents; it never cascades.** Children inherit the removed node's parents, deduplicated, and the removed node's own id is filtered out of what they inherit. This is not politeness: [`Stack::topo_order`](#stack-data-model) counts in-degree only over parents that resolve to a node, so a delete that merely dropped the node would leave the stack quietly describing an edge that no longer exists, and nothing would ever report it.
- **A rejected mutation writes nothing.** All four writers validate against a candidate `Stack` — existence, self-parenthood, repeated ids, cycles — before calling `update_stack_atomic`.
- **A node's PR is still identified by `pr_status.url`.** `pr_read` / `pr_comments` / `pr_adopt` resolve a pull number through `pr_number_from_status_url`; `StackNode` gained no PR-number field, so there is one source of truth and no migration. A node that records no URL is not addressable by `node_id` and says so rather than guessing.
- **Adoption cannot double-track a PR.** It is refused when the PR's head branch is already bound to a node *or* when any node already records that pull number.

Two limitations are part of the contract rather than defects:

- **A search hit carries no head or base branch.** `GET /search/issues` does not report them, so `base:` works as a search qualifier but the branch names are absent; the agent follows up with `pr_read`.
- **No thread is reported as resolved.** Thread resolution is exposed only by GitHub's GraphQL API, so the REST-backed `pr_comments` emits no `resolved` field rather than guessing one.

A search is always scoped to the orchestrator's own repository: `repo:` and `is:pr` are injected by the tool, and the agent's `text` / `author` / `base` values are refused if they carry a `:` or whitespace — GitHub *ORs* repeated `repo:` qualifiers, so an unguarded value would read another repository with the operator's own credential.

### GitHub API surface

`GithubPrApi` trait (real implementation + mock transport for tests): `get_open_pr`, `merge_pr(number)`, `patch_pr_base(number, new_base)`, `create_pr(head, base, title, body)`, and `close_pr(number)`. Backed by shared curl helpers `curl_github_patch_json`, `curl_github_post_json`, and `curl_github_put_json` in `github_rest_common.rs`.

`GithubPrInsightApi` *(added 2026-07-30)* is a **sibling** trait, not more methods on `GithubPrApi`: `get_pr`, `list_pr_files`, `list_check_runs`, `list_reviews`, `list_review_comments`, `list_issue_comments`, `search_prs`, and the one write `patch_pr_title_body`. Kept separate because the eight hand-written fakes implementing `GithubPrApi` care only about lifecycle operations and would otherwise all need stubs for reads they never exercise. `RealGithubPrApi` implements both. `/search/issues` is not under `/repos/{owner}/{repo}/`, so `github_rest_common.rs` gained `curl_github_get_json_absolute_path` alongside the repo-scoped helpers.

Response shaping lives in `orchestrate_pr_stack::pr_insight` (`read_pr`, `read_pr_comments`, `search_repository_prs`, `pull_number_for_node`), so every shape the agent sees is testable against a fake; the `tddy-tools` tool bodies only resolve environment and serialize. The `curl` request bodies themselves have no automated coverage — the transport shells out to `curl` against a hardcoded API base and cannot be intercepted, the same gap the `GithubPrApi` methods have.

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

### Operator-selectable base branch for a diamond node *(added 2026-07-27)*

A planned PR with multiple non-merged parents (a diamond / merge node) used to base its child worktree off whichever parent the resolver walked **first** — `Stack::base_ref_for_spawn` / the web `resolveStackBase` iterate `node.parents` in list order and return the first non-merged ancestor's `origin/<branch>`. Every parent is a legitimate base ref, but the operator had no control to pick a different one, and the dialog's `baseBranchLabel` was display-only (never sent to the daemon).

The Start-session dialog opened from a planned-PR row now renders a **"Base branch"** `<select>` (test id `create-session-base-branch-select`), shown when `initialValues.stackParent` is set, `peerMode` is false, and the option list is non-empty. Options and pre-selection are resolved together by **`baseBranchChoice(node, nodes, defaultBranch): { options, selected }`** (`packages/tddy-web/src/components/sessions/prstack/`), which orders the stack's own branches via the pure helper **`prioritiseBaseBranchOptions(node, nodes): string[]`**:

1. **Direct dependency branches** — walk `node.parents` in order; for each parent that is **not merged** and **owns a `branch`**, include that branch. Order the result by the dependency's own depth in the stack DAG (longest path from a root, deepest first), ties broken by the order in `node.parents` (stable). A merged parent contributes nothing (its ref may be gone and `effective_base_refs` collapses past it); a branchless parent contributes nothing (no ref to offer — the spawn gate would refuse it anyway).
2. **Other materialized stack branches** — every node with a `branch`, excluding the node itself, its descendants (basing onto a descendant would create a cycle), merged nodes, and any branch already listed in step 1. Appended in stack node order. De-duplicated by branch name (first-seen wins).
3. **The project's default branch** — appended by `baseBranchChoice` itself, last (from `ProjectEntry.main_branch_ref`), unless a stack branch already names it. It is the deliberate escape from the stack: a node **repointed onto the default branch** must be able to show that base and re-pick it. *(added 2026-07-30)*

The selected value is the node's **derived stack base** — `deriveStackBaseBranch`, the same value the branch-intent caption states — and is sent on the existing **`StartSessionRequest.selected_integration_base_ref`** field. It is always one of the offered options, so nothing can be submitted that the operator cannot see selected. *(Until 2026-07-30 the pre-selection was `options[0]` and the default branch was not offered at all: a planned PR repointed onto the default branch showed the caption `origin/master` while the picker pre-selected — and submitted — an unrelated stack branch, silently undoing the repoint.)* When the stack offers no branch of its own (a root node with no other materialized branches) the selector is hidden and the field is sent empty — preserving the root → default-branch behavior, which the daemon resolves for itself. Peer-mode spawn (which reuses the orchestrator's worktree via `repo_path`) hides the selector and sends an empty field regardless. A legacy project that stores no `main_branch_ref` still gets the default option, as the empty ref under the label *"project default"*.

**Ordering rationale — depth, not distance.** For a diamond where `PR3` depends on `[PR2, PR1]` and `PR2` depends on `PR1`: both direct parents are at distance 1, but `PR2` sits **deeper** in the DAG (itself depends on `PR1`). Basing onto `PR2` gives the operator `PR1`'s changes too, so `PR2` is the more specific, more complete base and is listed first. Depth is the longest path from a root to the dependency; ties (two roots, as in the `attach-start` case) break by `node.parents` order.

The daemon honors the explicit choice at the spawn seam — see [Git integration base ref — Operator-chosen base at the spawn seam](git-integration-base-ref.md#operator-chosen-base-at-the-spawn-seam-added-2026-07-27). The existing `baseBranchLabel` (the branch-intent caption, from `deriveStackBaseBranch`) is unchanged and independent of the selector.

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

The session's **context documents** are surfaced to the web via `SessionEntry` proto field 27, `repeated SessionContextDoc context_docs`, where `SessionContextDoc { key, basename, path, description, exists, kind, size_bytes }` and `kind` is `SessionContextDocKind` (`MANIFEST` or `ATTACHMENT`). Enrichment populates the list in `session_list_enrichment.rs` (alongside `orchestrator_session_id`):

- **Manifest rows** — recipe-owned planning artifacts (`exploration.md`, `stack-plan.yaml`, `pr-stack-plan.md`, the two `stack-status.*` files). `session_context_docs::context_docs_for_session` joins the manifest's `known_artifacts()` with a per-key human `description` (`SessionArtifactManifest::artifact_doc_descriptions()`; `PrStackRecipe` provides a one-liner per artifact), an on-disk existence flag, and `size_bytes`, resolving each `path` under `session_artifacts_root` (`session_dir/artifacts/`). A blank or unknown recipe yields an empty *manifest* half.
- **Attachment rows** — user-attached files under `artifacts/attachments/`, listed after the manifest rows with `kind = ATTACHMENT`. Attachments are recipe-independent: a blank or unknown recipe still lists them. Layout and store contract: [session-attachments.md](session-attachments.md).
- `session_context_docs::read_session_context_doc_utf8(recipe_name, session_dir, basename)` reads a doc's contents, allowlisted to the **manifest** basenames with a canonicalize-and-contain guard rooted at the artifacts dir (a non-manifest / attachment basename or a traversal segment → `PermissionDenied`). It mirrors the guard shape in `session_workflow_files.rs`. The wire RPC that exposes this reader is added by the web-facing follow-up that consumes it (the **Docs** tab and child "Start session" prompt references).

### New-session screen: recipe dropdown and parent picker

`CreateSessionPane` (`packages/tddy-web/src/components/sessions/`) gains two changes for tool sessions:

- **Recipe dropdown** — the free-text recipe input is replaced with a `<select>` listing the canonical recipe set, including the unified **`pr-stack`** (the legacy `plan-pr-stack` / `orchestrate-pr-stack` entries are no longer offered in the dropdown — new sessions always start as `pr-stack` — though both CLI names still resolve if typed directly). `pr-stack` is included in the `--recipe` `value_parser` in `packages/tddy-coder/src/run.rs` alongside the two legacy aliases.
- **Parent stack picker** — a new optional "Parent orchestrator" `<select>` (tool type only) that lists existing sessions identified as PR-stack orchestrators (sessions whose `recipe` is `pr-stack` — or a legacy alias — and that are not themselves children of another orchestrator; see `prStackOrchestrators()` in `packages/tddy-web/src/utils/stackParents.ts`). Selecting a parent passes `stack_parent` (proto `StartSessionRequest` field 15) to the daemon, which threads it through `SpawnOptions` → `--stack-parent <id>` CLI arg.

### Per-workflow session views: the PR-Stack Chat Screen

Once a `pr-stack` session is selected in the session drawer, the main pane opens a dedicated **PR-Stack Chat Screen** instead of the terminal — a full-width chat window backed by a remote Presenter (over the existing `TddyRemote.Stream` RPC) plus a dismissible **Planned PRs panel** on the right listing the planned PRs, with a **"Start session"** CTA per startable node (*since 2026-07-26*; it was a fixed half-width left pane before). The CTA opens the Start-session dialog, which for a diamond / multi-parent node renders a **"Base branch"** selector so the operator can choose which parent branch the child worktree bases off — see [Operator-selectable base branch for a diamond node](#operator-selectable-base-branch-for-a-diamond-node-added-2026-07-27). This chat *is* the `orchestrate` free-prompting loop: the developer types instructions ("merge n1", "repoint the dependents", "what needs action?") and the agent responds and calls the [PR-management tools](#pr-management-tools). Full UI spec: [Session drawer § Per-Workflow Session Views](../web/session-drawer.md#per-workflow-session-views).

Each planned-PR row (`PlannedPrRow.tsx`) renders an **internal-status badge** next to the existing phase chip, colored by `internal_status.kind` (e.g. amber `needs-repoint`, red `has-conflicts`, green `ready-to-merge`), with `internal_status.note` as hover text. The badge is parsed from the `internal_status` field carried inside `SessionEntry.stack_plan_json` (`stackPlan.ts::parseStackPlan`).

### Manually adding a planned PR

Until now the only way to change an orchestrator's planned-PR list was **chat-driven refinement** (§ pr-stack recipe, above): the operator asks the agent to re-plan, the agent re-emits `stack-plan.yaml`, and the host overwrites the whole node list (`reseed_stack_from_plan_if_unspawned`) — a round trip through the LLM, and an all-or-nothing rewrite that's refused once any node owns a branch or a spawned child session.

The PR-Stack Chat Screen's planned-PR list gains a **direct, deterministic path** that doesn't touch the LLM: a "New planned PR" form lets the operator manually add a single node — title, description, optional branch suggestion, optional child recipe, and a **multi-select ancestor picker** listing the orchestrator's existing planned-PR nodes (its chosen ancestors become the new node's `parents`, i.e. `StackNode.parents` — see [Stack data model](#stack-data-model)).

- **RPC:** `ConnectionService.AddPlannedPr(AddPlannedPrRequest) -> AddPlannedPrResponse` (`connection.proto`). Request carries `session_id` (the orchestrator), `title`, `description`, `branch_suggestion`, `parents` (chosen ancestor node ids), and `child_recipe`. Response carries `stack_plan_json` — the same wire shape as `SessionEntry.stack_plan_json` (field 23) — so the web reuses the existing `parseStackPlan` parser rather than a second message schema.
- **Semantics:** appends exactly one `StackNode` (`branch: None`, `session_id: None`, `pr_status: None` — stays planned, does not spawn a session or create a branch; only `branch_suggestion` is set) to `Changeset.stack` via `update_stack_atomic`. Unlike chat refinement, this **never touches existing nodes** and is not gated on whether other nodes have already spawned — it's additive only. The node id is server-assigned (never client-supplied). Rejects (without writing) when a chosen ancestor doesn't resolve to an existing node id, or when appending would introduce a cycle (checked via `Stack::topo_order`, same guard as the plan-time validator).
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

- [PR-Stack live status & repoint](pr-stack-live-status.md) — the branch as the durable link key; branch→session/PR resolution and live PR status in the web view; the Repoint control; and the sequence-respecting spawn base (`Stack::base_ref_for_spawn`).
- [Session drawer](../web/session-drawer.md) — session drawer screen layout, create session, recipe field, grouping, and the [Per-Workflow Session Views](../web/session-drawer.md#per-workflow-session-views) / [PR-Stack Chat Screen](../web/session-drawer.md#pr-stack-chat-screen) sections.
- [Git integration base ref (worktrees)](git-integration-base-ref.md) — session chaining, `spawn_chain_child_worktree`, worktree base-ref validation.
- [Session layout](session-layout.md) — session directory structure, `changeset.yaml`, artifact paths.
- [Workflow recipes](workflow-recipes.md) — `WorkflowRecipe` trait, recipe resolution, `approval_policy`, shipped recipes.
