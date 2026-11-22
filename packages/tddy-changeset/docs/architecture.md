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
| `session_work_queue` | `SessionWorkQueues`, `SessionWorkQueueRecord`, `SessionWorkQueueError`: the work queue a session owns at `<sessions>/<session_id>/work-queue.yaml` — `create_for_session` (at start, once), `read`, `update`, `list_for_project` — written under a per-record in-process lock with an atomic rename; `starts_with_work_queue` (asked for, or an orchestrator-role start — the one role-aware rule), `NO_WORK_QUEUE_HINT`, and `drop_old_project_stack_records`, which deletes the `<projects>/*/stacks/` directories at daemon start |
| `work_queue_store` | `WorkQueueStore`, `WorkQueueRecord`, `WorkQueueStoreError`: the project-scoped view over those queues (`read` / `update` / `list` by `(project_id, queue_id)`, where `queue_id` is the owning session's id); refuses a queue of another project and creates nothing |
| `work_queue_fixtures` (`cfg(test)` or feature `test-support`) | the sessions layout a stack needs in tests — `a_stack_host`, `a_stack_session_dir`, `write_changeset_with_stack`, `stack_on_disk` — for this crate and other crates' `[dev-dependencies]` |
| `branch_worktree_intent` | `BranchWorktreeIntent`: whether a session works on a new branch cut from a base or on a selected existing one |
| `session_lifecycle` | the unified session directory and its id rules |
| `session_github_source` | the session's recorded GitHub source: `SessionGithubSource { Vault, LocalGh }`, `write_session_github_source`, `read_session_github_source` (`Ok(None)` when none was recorded) |
| `session_provider_account` | the provider account a session runs on: `SessionProviderAccount { account_id, model }`, `write_session_provider_account`, `read_session_provider_account` (`Ok(None)` when the session runs on its vendor) |
| `tab_roster` | a session's **tab roster**: `TabRoster`, `TabEntry` (`terminal_id`, `launch`), `LaunchNode` (`kind`, string `properties`) with the kind and property-key constants (`AGENT_KIND`, `SHELL_KIND`, `TMUX_KIND`, `TMUX_ATTACH_KIND`, `HARNESS_PROPERTY`, `CONVERSATION_ID_PROPERTY`, `SESSION_PROPERTY`, `OS_USER_PROPERTY`), `read_tab_roster` (an absent file reads as an empty roster) and `write_tab_roster` (atomic) for `<session dir>/tabs.yaml`. A node kind, property key, entry key or top-level key it does not know is read and written back verbatim. A file of its own because `.session.yaml` is `deny_unknown_fields` and is rewritten by unlocked read-modify-writes; its one writer is `tddy-cli-sessions`'s `TabRosterStore` |
| `session_metadata`, `session_agent`, `session_activity`, `session_label`, `session_display_name`, `session_participant_metadata`, `session_context` | the per-session files beside the changeset |
| `agent_activity` | the agent tool-call activity log |
| `pending_question` | the question a session is waiting on: `PendingQuestion`, `write_pending_question`, `clear_pending_question`, `read_pending_question` |
| `elapsed_format`, `source_path` | small formatting and path helpers |

## Session GitHub source (`session_github_source`)

The file is `github-source.yaml` in the session directory, kept **outside** `.session.yaml` because
that file rejects unknown fields (an older daemon would refuse the session) and because the daemon, not
the child, owns it. The daemon writes it only when a start *carried* a source
(`StartSessionRequest.github_source`); a start that named none records nothing, so a resume re-reads the
host's project row, as an account assignment is re-read. See
[session identity](../../tddy-session-lifecycle/docs/session-identity.md#which-source-a-session-uses).

## Session provider account (`session_provider_account`)

The file is `provider-account.yaml` (`SESSION_PROVIDER_ACCOUNT_FILENAME`) in the session directory,
outside `.session.yaml` for the same two reasons as `github-source.yaml`. Only a session started on a
provider account writes it, before its harness is spawned. It holds the account **id** and the model,
never a key — a resume re-reads the key from the owner's vault, so a key rotated or removed since the
start is what the resume sees. A missing file is `Ok(None)`; an unreadable or malformed one is
`WorkflowError::ChangesetInvalid`. See
[Claude Code CLI session § Persistence and resume](../../../docs/ft/daemon/claude-cli-session.md#persistence-and-resume).

## Session name (`session_display_name`)

`SessionMetadata.display_name` is the name an operator, an orchestrator or the session's own agent
gave the session; absent (never written) for an unnamed session, so an unnamed record reads exactly
as one from before names existed. Every writer goes through `normalize_display_name`: surrounding
whitespace is trimmed, an empty name is `None` (the name is cleared), and a name over
`DISPLAY_NAME_MAX_CHARS` (80 characters, not bytes) or holding a control character is refused
(`DisplayNameError::TooLong { chars, max }`, `ControlCharacter`) rather than cut or cleaned.
`rename_session(session_dir, raw)` records the normalised name, or clears it, and changes no other
field — `updated_at` included, since naming a session is not activity in it — and answers the name as
recorded; a directory with no readable record is `RenameSessionError::Unreadable`.

`session_label::session_display_label(display_name, repo_path, workflow_goal, session_id)` puts the
name first (trimmed; blank counts as none), ahead of the basename of `repo_path`, `workflow_goal` and
the short id. Pinned by `tests/session_display_name.rs` and `tests/session_display_label_acceptance.rs`.

## Harness resume token (`session_metadata`)

`SessionMetadata.harness_resume_token: Option<String>` (`#[serde(default, skip_serializing_if =
"Option::is_none")]`, so a session without one writes no field) is the handle a CLI harness printed for
continuing its conversation when it last exited — Warp's `warp --resume <TOKEN>`, read back off the
terminal by `tddy-agent-launch`'s resume-token recorder. A resume passes it to the harness as
`CliLaunch.conversation_id`. It is absent until a run has printed one, and for harnesses that print none;
it is generic, and separate from Cursor's `cursor_chat_id`, which is minted before the spawn. Unit tests
in `session_metadata.rs`: `a_session_without_a_resume_token_writes_no_resume_token_field`,
`a_recorded_resume_token_round_trips_through_session_yaml`.

## Changeset (`changeset`)

- **Changeset**: Unified manifest in plan directory. Replaces `.session` and `.impl-session`. Contains name, initial_prompt, clarification_qa, models, sessions (with system_prompt_file per session), state, artifacts, discovery, worktree, branch, branch_suggestion, worktree_suggestion, repo_path, optional **effective_worktree_integration_base_ref** (remote-tracking ref used to create the worktree), optional **worktree_integration_base_ref** (user-selected chain-PR base when present).
- **SessionEntry**: id, agent, tag, created_at, system_prompt_file (path to system prompt for this session).
- **ClarificationQa**: Question and answer pairs from planning clarification.
- **read_changeset / write_changeset**: Load and persist changeset.yaml.
- **append_session_and_update_state**: Add session (agent from backend.name(), id, tag, system_prompt_file); update workflow state.

**PR-stack DAG (`WorkQueue`, owned by a session).** The queue lives in its owning session's directory (`work-queue.yaml`), not in `changeset.yaml`; `read_session_stack` reads it (`None` for a session that owns none) and `update_stack_atomic` writes it, refusing a session that owns none with `NO_WORK_QUEUE_HINT` — no write creates a queue. A stack progresses on **branches, not on sessions** — a branch can be built on whether or not a session is still attached to it. See [pr-stacking.md](../../../docs/ft/coder/pr-stacking.md).

- **Stack::base_ref_for_spawn(node_id, stack_bottom_base) -> Result<String, WorkflowError>**: a node's spawn base — the nearest non-merged ancestor's `<remote>/<branch>` (the remote the project resolves, not necessarily `origin`), else `stack_bottom_base`. Refuses (`ChangesetInvalid`) when a non-merged parent owns no `branch`, naming the parent and its missing branch. A parent's `session_id` is not consulted.
- **Stack::effective_base_refs(node_id, stack_bottom_base)**: counts only branch-bearing non-merged parents. A branchless parent contributes nothing — it is never given a synthesized `<remote>/<node_id>` ref.
- **resolve_stack_node_branch(sessions_root, node) -> Option<String>**: the node's own `branch`, else the `branch` recorded in its child session's changeset — the *fallback* route, for a node linked before its branch was known. A missing session directory resolves to `None`, never an error.
- **read_stack_with_resolved_branches(sessions_root, orchestrator_session_id) -> Result<Option<Stack>, WorkflowError>**: the orchestrator's stack with every node's `branch` hydrated through the resolver; `Ok(None)` when the session carries no stack. The hydrated copy is read-only — persisting it would write a fallback-derived branch onto a node that never recorded one.
- **link_work_item_to_session(orchestrator_dir, node_id, child_session_id, branch)**: record the branch a spawn created (and its session) on the node.
- **`branch` vs `branch_suggestion`**: `branch` means "a branch that exists"; `branch_suggestion` is a planned name that never satisfies the spawn gate. Planning leaves `branch = None`.
- **`WorkItem.display_order: Option<u32>`**: the operator-visible row position, persisted so it is independent of the DAG. A merge, a repoint or a re-parenting rewrites `parents` and therefore the topology, and rows must not move under the operator when they do. Additive and omitted when unset.
- **Stack::display_order() -> Vec\<String\>**: the render order, beside `topo_order`. Sort key `(display_order.unwrap_or(u32::MAX), topological index, node_id)`. **Never fails** — it is on a render path, so a cycle degrades the tie-break to declaration order rather than erroring, and no node is ever dropped. Numbering happens on *write* (`work_queue::assign_missing_display_order`), never on read: backfilling inside `read_changeset` would make the value returned differ from the bytes on disk.

## Pending question (`pending_question`)

- **`pending-question.json`** (`PENDING_QUESTION_FILE`) in the session directory holds the one
  question the session waits on: `PendingQuestion {kind, header, question, options, allow_other}`,
  `kind` being `select`, `multi_select`, `text` or `permission`. `.session.yaml`'s
  `pending_elicitation` says *that* the session waits; this file says *on what*. It is its own file
  rather than a `SessionMetadata` field, which would have touched every exhaustive metadata literal.
- **write_pending_question** replaces any earlier question by writing a sibling temp file and
  renaming it over the real one, so a reader sees the whole old or the whole new question.
  **clear_pending_question** removes it and is idempotent. **read_pending_question** answers
  `Ok(None)` when the file is absent.
- Two writers keep it: a `tool` session's own process from its presenter's mode
  (`tddy-coder` `session_participant::pending_question`), and the daemon's `ReportSessionStatus`
  for a claude-cli session (`tddy-session-activity` `service/hook_reports.rs`). The reader is
  `ReadSessionActivity` (`tddy-tool-engine` `session_follow`).

## Agent activity (`agent_activity`)

- **AgentActivityRecord**: the shared, single cross-crate shape for one agent tool call — `call_id` (correlates the `running` and terminal rows), `tool_name`, `input` (structured `serde_json::Value`), `status` (`running`/`completed`/`error`), `result` (structured `serde_json::Value`; `Null` until terminal), `error_message`, `started_unix_ms`, `completed_unix_ms`, `source` (`coder`/`cursor-cli`/`claude-cli`/`sandbox`). `input`/`result` cross the wire as `google.protobuf.Value` (via `tddy_service::agent_activity_to_proto`). Modeled on `tddy-daemon/src/tool_call_log.rs` but placed in this crate, beneath every host, so every host writes the same record.
- **append_agent_activity / read_agent_activity**: append-only JSONL writer + reader for the per-session `agent-activity.jsonl` log (sibling of `tool-calls.jsonl`). The read side **coalesces by `call_id`** field by field (a later row supersedes what it reports, but a `PostToolUse` row's missing start time and input are kept from the `PreToolUse` row; first-seen order preserved) then applies a 500-record tail cap; malformed lines are skipped. **read_all_agent_activity** is the same read without the cap, for a reader that pages through the whole history (`ReadSessionActivity`). This is the agent's own tool loop, distinct from the human-triggered `ExecuteTool` web-invoke log. **parse_activity_json** turns a hook-supplied JSON string into the structured `Value` field (empty → `Null`, else parse-or-`Value::String`), shared by every capture seam. **read_all_sequenced_agent_activity** returns each call as a `SequencedActivity` with its `log_seq` — the 1-based line of its latest row in the append-only log: unique, rising, the same on every read, and moved forward by a call's completion row — which `ReadSessionActivity` pages by (not `activity_seq`, the session room's tick).
