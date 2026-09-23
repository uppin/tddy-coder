# tddy-changeset architecture

## Overview

The changeset model and the session metadata stored beside it: `changeset.yaml`, the PR-stack DAG
an orchestrator session carries, the unified session directory, and each session's agents, activity
and labels.

### Dependency rule

| Crate | Why |
|---|---|
| `tddy-workflow` | the shared vocabulary (`GoalId`, `WorkflowState`, `ClarificationQuestion`) |
| `tddy-session-store` | `atomic_file`, `error` and `output` — named at their old `crate::` paths by private imports in `lib.rs` |
| `tddy-graph` | the graph `Context` a stored workflow merges into |

It does **not** depend on the workflow engine or the backends. The one changeset function that
answered through a `WorkflowRecipe`, `start_goal_for_session_continue`, lives in
[`tddy-workflow-engine`](../../tddy-workflow-engine/docs/architecture.md) for that reason.
`packages/tddy-core/tests/core_facade_shape.rs` pins both the "below the engine" rule and the size
budget.

`tddy-core` re-exports this crate whole (`pub use tddy_changeset::*;`), so every `tddy_core::{changeset, session_metadata, session_lifecycle, agent_activity, …}::…` path consumers name resolves unchanged. New code should name `tddy_changeset` directly.

## Modules

| Module | Owns |
|---|---|
| `changeset` | `changeset.yaml`: `model`, `stack` (the PR-stack DAG), `io` (atomic reads and writes), `merge` (what a stored changeset means for the run about to start) |
| `branch_worktree_intent` | `BranchWorktreeIntent`: whether a session works on a new branch cut from a base or on a selected existing one |
| `session_lifecycle` | the unified session directory and its id rules |
| `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_participant_metadata`, `session_context` | the per-session files beside the changeset |
| `agent_activity` | the agent tool-call activity log |
| `elapsed_format`, `source_path` | small formatting and path helpers |

## Changeset (`changeset`)

- **Changeset**: Unified manifest in plan directory. Replaces `.session` and `.impl-session`. Contains name, initial_prompt, clarification_qa, models, sessions (with system_prompt_file per session), state, artifacts, discovery, worktree, branch, branch_suggestion, worktree_suggestion, repo_path, optional **effective_worktree_integration_base_ref** (remote-tracking ref used to create the worktree), optional **worktree_integration_base_ref** (user-selected chain-PR base when present).
- **SessionEntry**: id, agent, tag, created_at, system_prompt_file (path to system prompt for this session).
- **ClarificationQa**: Question and answer pairs from planning clarification.
- **read_changeset / write_changeset**: Load and persist changeset.yaml.
- **append_session_and_update_state**: Add session (agent from backend.name(), id, tag, system_prompt_file); update workflow state.

**PR-stack DAG (`Changeset.stack`).** A stack progresses on **branches, not on sessions** — a branch can be built on whether or not a session is still attached to it. See [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md).

- **Stack::base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>**: a node's spawn base — the nearest non-merged ancestor's `<remote>/<branch>` (the remote the project resolves, not necessarily `origin`), else `stack_bottom_base`. Refuses (`ChangesetInvalid`) when a non-merged parent owns no `branch`, naming the parent and its missing branch. A parent's `session_id` is not consulted.
- **Stack::effective_base_refs(node_id, stack_bottom_base)**: counts only branch-bearing non-merged parents. A branchless parent contributes nothing — it is never given a synthesized `<remote>/<node_id>` ref.
- **resolve_stack_node_branch(sessions_root, node) -> Option<String>**: the node's own `branch`, else the `branch` recorded in its child session's changeset — the *fallback* route, for a node linked before its branch was known. A missing session directory resolves to `None`, never an error.
- **read_stack_with_resolved_branches(sessions_root, orchestrator_session_id) -> Result<Option<Stack>, WorkflowError>**: the orchestrator's stack with every node's `branch` hydrated through the resolver; `Ok(None)` when the session carries no stack. The hydrated copy is read-only — persisting it would write a fallback-derived branch onto a node that never recorded one.
- **link_stack_node_to_child_session(orchestrator_dir, node_id, child_session_id, branch)**: record the branch a spawn created (and its session) on the node.
- **`branch` vs `branch_suggestion`**: `branch` means "a branch that exists"; `branch_suggestion` is a planned name that never satisfies the spawn gate. Planning leaves `branch = None`.
- **`StackNode.display_order: Option<u32>`**: the operator-visible row position, persisted so it is independent of the DAG. A merge, a repoint or a re-parenting rewrites `parents` and therefore the topology, and rows must not move under the operator when they do. Additive and omitted when unset.
- **Stack::display_order() -> Vec\<String\>**: the render order, beside `topo_order`. Sort key `(display_order.unwrap_or(u32::MAX), topological index, node_id)`. **Never fails** — it is on a render path, so a cycle degrades the tie-break to declaration order rather than erroring, and no node is ever dropped. Numbering happens on *write* (`pr_stack::assign_missing_display_order`), never on read: backfilling inside `read_changeset` would make the value returned differ from the bytes on disk.

## Agent activity (`agent_activity`)

- **AgentActivityRecord**: the shared, single cross-crate shape for one agent tool call — `call_id` (correlates the `running` and terminal rows), `tool_name`, `input` (structured `serde_json::Value`), `status` (`running`/`completed`/`error`), `result` (structured `serde_json::Value`; `Null` until terminal), `error_message`, `started_unix_ms`, `completed_unix_ms`, `source` (`coder`/`cursor-cli`/`claude-cli`/`sandbox`). `input`/`result` cross the wire as `google.protobuf.Value` (via `tddy_service::agent_activity_to_proto`). Modeled on `tddy-daemon/src/tool_call_log.rs` but placed in this crate, beneath every host, so every host writes the same record.
- **append_agent_activity / read_agent_activity**: append-only JSONL writer + reader for the per-session `agent-activity.jsonl` log (sibling of `tool-calls.jsonl`). The read side **coalesces by `call_id`** (later row supersedes, first-seen order preserved) then applies a 500-record tail cap; malformed lines are skipped. This is the agent's own tool loop, distinct from the human-triggered `ExecuteTool` web-invoke log. **parse_activity_json** turns a hook-supplied JSON string into the structured `Value` field (empty → `Null`, else parse-or-`Value::String`), shared by every capture seam.
