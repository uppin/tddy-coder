# 2026-10-25 — Close the `#coagents` stack's functionality TODOs (sweep)

`#coagents` 9/9 (sweep node, top of the stack), on #727 (`feature/coagents/web-approvals`).
**Branch:** `feature/coagents/sweep`.
**Packages:** tddy-service (proto + regenerated TS), tddy-discovery, tddy-coding-subagent,
tddy-agent-backend, tddy-session-agents, tddy-session-lifecycle, tddy-tools, tddy-daemon-rpc (tests),
tddy-daemon (start-up wiring).

The six functionality TODOs the stack deferred to its backlog, each approved by the developer for
this node. Code-issue records and the test-only backlog entries are out of scope (see
`## Boundaries`).

## Prerequisites

| Backlog entry | Verdict |
|---|---|
| [`2026-10-24-coding-agent-cancel-does-not-reach-session-cancel`](../todo/2026-10-24-coding-agent-cancel-does-not-reach-session-cancel.md) | ✅ RESOLVED HERE — item 1 |
| [`2026-10-25-cancel-agent-conversation-does-not-check-the-session`](../todo/2026-10-25-cancel-agent-conversation-does-not-check-the-session.md) | ✅ RESOLVED HERE — item 2 |
| [`2026-10-24-acp-permission-request-drops-tool-kind`](../todo/2026-10-24-acp-permission-request-drops-tool-kind.md) | ✅ RESOLVED HERE — item 3 |
| [`2026-10-25-headless-approval-route-residuals`](../todo/2026-10-25-headless-approval-route-residuals.md) | ✅ RESOLVED HERE — item 4 |
| [`2026-10-25-conversation-status-row-is-durable-only`](../todo/2026-10-25-conversation-status-row-is-durable-only.md) | ✅ RESOLVED HERE — item 5 |
| [`2026-10-25-headless-cursor-codex-oz-get-no-mcp-config`](../todo/2026-10-25-headless-cursor-codex-oz-get-no-mcp-config.md) | ✅ RESOLVED HERE for the MCP config — item 6. The entry's second half (prove a withdrawal mode so `confinement_for` can leave `CannotWithdraw`) is **not** done here: the wrap narrows the entry to it rather than deleting it |

## Responsibility

### 1. `subagent_cancel` on a coding agent cancels the turn and keeps the conversation

Today `CancelAgentConversation` removes every conversation, so a coding agent's process and context
are lost on a cancel. `CodingAgentSession::cancel_handle` (real ACP `session/cancel`) is unreachable
from the conversation table (`TODO(acp-subagent n3)` in `tddy-coding-subagent/src/session.rs`).

**Shape.** Cancelling a *turn* and closing a *conversation* are different requests, and both
callers exist: `subagent_cancel` wants the first, `subagent_end` and detach want the second (both
reach the daemon through `CancelAgentConversation` today). So the wire says which one is asked:

- `CancelAgentConversationRequest.keep_conversation` (bool, field 6; 5 is reserved): cancel only the running turn
  when the conversation's agent can, and keep the conversation open. `false` (the default, and what
  every existing caller sends) closes it exactly as today.
- `CancelAgentConversationResponse.conversation_kept` (bool, field 1): whether the daemon kept it.
  Only a session that can cancel its own turn keeps one; every other conversation (a model loop) is
  closed as before and answers `false`.
- `SubagentSession::turn_canceller() -> Option<Arc<dyn TurnCancelling>>` (tddy-discovery), the same
  shape as `permission_answerer`: captured beside the boxed session when the conversation opens, so
  a cancel reaches it while a turn holds the session's lock. `CodingAgentSession` returns its
  `CancelHandle`; clone placement's `CodingAgentConversationTurns` forwards it.
- `tddy-tools`' `subagent_cancel` sends `keep_conversation: true` for a daemon-run conversation and
  retires its own entry only when the daemon answers `conversation_kept: false`; `subagent_end`
  sends `false` and closes, as now.

The #722 behaviour stays: a request parked when the cancel arrives is answered `Cancelled` (and
recorded as answered by the canceller, as #727 records it), then `session/cancel` is sent.

**Acceptance criteria**
1. A `keep_conversation` cancel of a coding-agent conversation answers `conversation_kept: true`;
   the agent process is still running; the next prompt on the same conversation id succeeds.
2. A `keep_conversation` cancel of a turn paused on a permission request answers that request
   `Cancelled` and the turn ends `cancelled` (#722's contract, through the daemon).
3. A plain cancel (`keep_conversation: false`) still closes the conversation and stops the process
   (`ending_a_coding_agent_conversation_stops_its_process` unchanged).
4. A `keep_conversation` cancel of a model-loop conversation closes it and answers
   `conversation_kept: false`.
5. `subagent_cancel` on a daemon-run conversation sends `keep_conversation: true`; when the daemon
   keeps it, the conversation stays in the tool's open table and the reply says so.
6. The `TODO(acp-subagent n3)` marker is gone.

### 2. `CancelAgentConversation` checks the conversation is the caller's session's

`cancel_agent_conversation` authenticates for `req.session_id` and then removes `req.conversation_id`
by id alone.

**Acceptance criteria**
1. A cancel naming a conversation open on another session answers `NOT_FOUND`, worded as for an
   unknown id, and removes nothing: the conversation is still open on its own session.
2. The same check covers the `keep_conversation` cancel of item 1.

### 3. A coding subagent's permission request carries the ACP tool kind

`tddy_agent_backend::PermissionRequest` drops `tool_call.fields.kind`, so `view_of` reports `other`
for every request (`FIXME(permission-yield)` in `parked_permissions.rs`).

**Shape.** `PermissionRequest.kind: Option<String>` — ACP's wire name of the kind (`edit`,
`execute`, `read`, …), the same spelling `PermissionOptionView.kind` uses for an option's kind;
`None` when the agent named none. Filled in `acp_driver/mapping.rs::permission_request`;
`view_of` passes it through, `other` when `None`. A headless approval
(`ApprovalRoutes::request`) names no ACP kind and passes `None`.

**Acceptance criteria**
1. `mapping::permission_request` carries the request's `tool_call.fields.kind` as its wire name.
2. `view_of` reports that kind as `PermissionToolCall.kind`, and `other` when the request has none.
3. The FIXME is gone.

### 4. A headless conversation's approval route and files do not outlive a failed open or a crash

`headless_plan` registers the route's launch half, then writes `headless/<conversation>/mcp.json`;
only `AgentRoster::conversation_closed` removes either.

**Shape.**
- Inside the plan: the route is registered only once everything the plan writes is written, so a
  plan that fails part-way leaves no route behind.
- After the plan: the open path disarms a guard only when the conversation is in the table. Any
  failure between the plan and the insert (the session start, the transcript the recorder opens)
  runs the same cleanup `conversation_closed` runs — the route and `headless/<conversation>/`.
- At start-up the daemon holds no conversation (the table is in memory), so the sweep removes every
  conversation directory under `<tddy_data_dir>/headless/`:
  `tddy_coding_subagent::launcher::sweep_headless_dirs(tddy_data_dir)`, called once by the daemon's
  runtime before it serves.

**Acceptance criteria**
1. A headless plan whose MCP config cannot be written leaves no route registered for the
   conversation (an approval for it is refused as *not open on this daemon*).
2. An open that fails after the plan (its transcript cannot be opened) leaves no route and no
   `headless/<conversation>/` directory.
3. `sweep_headless_dirs` removes every conversation directory under `headless/` and leaves the rest
   of the data directory alone; a missing `headless/` is not an error.

### 5. A conversation's status row is published live and marked as a status row

`conversation_status::record` appends to `agent-activity.jsonl` only (`TODO(web-approvals)`), and the
row is told apart from the agent's own calls only by its tool name `subagent_conversation`.

**Shape.** How the row is consumed today:
- `StreamAcpReplay` (the web's conversation bubble) folds it through
  `subagent_conversation_replay::conversation_row_entry`, which keys on the tool name.
- `StreamSessionActivity` (raw activity, `useSessionActivity.ts`) returns it as an ordinary
  `AgentActivityRecord`; no consumer reads `source`.

So the marker is the record's own provenance field, which every raw consumer already receives and
none filters on: `source = "subagent-status"` (`CONVERSATION_ROW_SOURCE` in
`tddy_service::subagent_conversation_replay`). No proto change. `conversation_row_entry` keys on the
source, so an agent's own call that happens to be named `subagent_conversation` is not read as a
status row. The row is published on the daemon's `AgentActivityHub` after it is appended, the path
`ReportAgentActivity` takes, so a live `StreamSessionActivity` subscriber receives it at once.

**Acceptance criteria**
1. A live `StreamSessionActivity` subscriber receives the awaiting-user row while the deferred
   resume is still held, without a new snapshot.
2. Every status row carries `source = "subagent-status"`.
3. `conversation_row_entry` reads a record as a status row by its source, not its tool name.
4. The `TODO(web-approvals)` marker is gone.

### 6. Headless `cursor`, `codex` and `oz` are handed `tddy-tools` in each CLI's own shape

Today only `claude` gets an MCP config. Each shape below was confirmed before coding:

| CLI | Where | Shape | Confirmed from |
|---|---|---|---|
| `cursor-agent` | `<worktree>/.cursor/mcp.json` | `{"mcpServers": {"tddy-tools": {"command", "args", "env"}}}` | installed `cursor-agent` 2026.10.14 bundle: reads `<projectRoot>/.cursor/mcp.json` and `~/.cursor/mcp.json`, `mcpServers` keyed by name; `--approve-mcps` (already passed) |
| `oz` | `--mcp <path>`, an owner-only file under `headless/<conversation>/` | `{"tddy-tools": {"command", "args", "env"}}` — servers at the top level, no wrapper | `oz agent run --help` (installed 0.2026.10.21): `--mcp <SPEC>` takes a path or inline JSON; docs.warp.dev MCP reference: top-level server names; `command`+`args`, `env` valid with `command` |
| `codex` | `-c mcp_servers.tddy-tools.command=…`, `.args=[…]`, `.env_vars=[…]` | dotted-path TOML overrides; the values of `env` reach the CLI's process environment, and `env_vars` whitelists them by name for the server, so no credential is on the argv | codex source (`codex-rs/utils/cli/src/config_override.rs`: `-c` is `global = true`, dotted paths, TOML values; `cli/src/main.rs` prepends root overrides into `exec`); config reference: `mcp_servers.<id>.command/args/env/env_vars`. No `codex` binary is installed on this machine — only the Superset shim |

- `HeadlessCommand.mcp_server: Option<McpServerSpec>` — the tool server for a CLI that takes it
  inline (codex); its `env` pairs are added to `HeadlessCommand.env`.
- Cursor's file holds a credential inside the conversation's git worktree, so it is written owner-only
  and listed in the repository's `info/exclude`, and an existing `.cursor/mcp.json` in the worktree
  is refused, never overwritten (it could be tracked, and the agent could commit it).
- Approvals are **not** bound for the three: none has a permission-prompt tool, so none asks.
- `confinement_for` is unchanged; all three stay `CannotWithdraw`.

**Acceptance criteria**
1. A headless `cursor` launch writes `<worktree>/.cursor/mcp.json` in cursor's shape, mode `0600`,
   ignored by git through `info/exclude`; an existing one is refused, naming the file.
2. A headless `oz` launch writes an owner-only file in oz's shape and hands it as `mcp_config`
   (`--mcp <path>`).
3. A headless `codex` launch carries the tool server as `mcp_server`, with its environment in
   `env`; codex's argv carries it as `-c mcp_servers.tddy-tools.*` overrides with no environment
   value on the argv.
4. `cursor_runs_its_resolved_cli_and_takes_no_mcp_config_yet` is rewritten: its no-config assertions
   were the limit this item removes.

## Boundaries

- **No code-issue work.** Oversized files and complexity records stay deferred; nothing here splits,
  refactors or restructures for them, and `packages/*/docs/code-issues/` is not touched.
- **No test-only backlog entries**: the two-daemon suite, live adapter withdrawal, the socket-env
  race and ConversationWorktree over HTTP stay in `docs/dev/todo/`.
- **No withdrawal mode for cursor / codex / oz.** `confinement_for` is not changed; a managed
  session still refuses all three.
- **No web UI change.** The web keeps reading the status row through the replay fold; whether the raw
  activity pane hides `source = "subagent-status"` rows is not decided here.
- No change to how a model-loop conversation cancels.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are theirs; this PR changes them
only where its responsibility says so.

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #720 acp-driver | `AcpDriver`, `PermissionRequest`, `mapping::permission_request`, `HeadlessCommand` (via #726) | adds `PermissionRequest.kind` | change the driver's protocol handling |
| #721 acp-subagent | `CodingAgentSession`, `CancelHandle`, the coding-agent launcher | reaches `CancelHandle` from the conversation table | change how a coding agent is launched |
| #722 permission-yield | `ParkedPermissions`, `view_of`, cancel-while-pending answers `Cancelled` | keeps that cancel behaviour, fills `kind` | change the yield or answer contract |
| #724 clone-placement | `CodingAgentConversationTurns` | forwards `turn_canceller` through it | change placement |
| #726 headless-driver | `headless_plan`, `ApprovalRoutes`, `write_mcp_config`, `conversation_closed`, argv builders | adds the open guard, the sweep, and the three CLIs' MCP configs | change claude's headless shape or the driver's turn loop |
| #727 web-approvals | `conversation_status`, `CONVERSATION_ROW_TOOL_NAME`, `conversation_row_entry`, cancel actor recording | publishes the row live, adds its source marker | change the web replay or the deferral flow |

## Draft PR contract

Commit 1: this document. Commit 2 (red): the surfaces below with failing tests; green follows in this
same PR.

- proto: `CancelAgentConversationRequest.keep_conversation = 6`,
  `CancelAgentConversationResponse.conversation_kept = 1` (+ regenerated TS).
- `tddy_agent_backend::PermissionRequest.kind: Option<String>`.
- `tddy_agent_backend::HeadlessCommand.mcp_server: Option<McpServerSpec>`.
- `tddy_service::subagent_conversation_replay::CONVERSATION_ROW_SOURCE`.
- `tddy_coding_subagent::launcher::sweep_headless_dirs(&Path) -> io::Result<()>`.
- `tddy_discovery::subagent::{TurnCancelling, SubagentSession::turn_canceller}`.

### Red tests (commit 2)

| Item | Test | Fails in red because |
|---|---|---|
| 1 | `tddy-daemon-rpc/tests/coding_subagent_acceptance.rs`: `cancelling_a_coding_agents_turn_keeps_its_conversation_and_its_process`, `a_coding_agent_conversation_answers_its_next_prompt_after_its_turn_was_cancelled`, `cancelling_a_turn_paused_on_a_permission_request_answers_it_cancelled_and_keeps_the_conversation` | the cancel closes the conversation |
| 1 | `tddy-tools/tests/subagent_cancel_keeps_a_kept_conversation.rs` (both) | `subagent_cancel` sends `keep_conversation: false` and retires the conversation |
| 2 | `tddy-daemon-rpc/tests/session_agent_conversation_acceptance.rs`: `refuses_to_cancel_a_conversation_named_on_another_session` | the cancel succeeds |
| 3 | `tddy-agent-backend` `mapping::tests::a_permission_request_carries_the_tool_kind_the_agent_named`; `tddy-coding-subagent/tests/parked_request_reports_its_tool_kind.rs` | `kind` is `None` / `other` |
| 4 | `tddy-session-lifecycle` `headless_tests::a_headless_plan_whose_mcp_config_cannot_be_written_leaves_no_approval_route`; `tddy-daemon-rpc/tests/headless_failed_open_leaves_nothing_behind.rs`; `tddy-coding-subagent/tests/sweep_headless_dirs.rs` | the route and the directory stay; the sweep is a no-op |
| 5 | `tddy-daemon-rpc/tests/waiting_state_record_acceptance.rs`: `a_live_activity_subscriber_sees_the_conversation_waiting_on_the_user_at_once`, `every_status_row_is_marked_as_the_daemons_status_row`; `tddy-service/tests/conversation_row_source_marker.rs` | the row is not published; its source is `coder`; the fold keys on the tool name |
| 6 | `tddy-session-lifecycle` `headless_tests::a_headless_{cursor,oz,codex}_*`; `tddy-agent-backend` `argv::tests::codex_is_handed_its_mcp_server_as_config_overrides` | no config is written or carried |

Guards that already pass (they pin what must not change): `cancelling_the_turn_of_a_model_agent_closes_its_conversation_as_before`,
`ending_a_coding_agent_conversation_stops_its_process`, `codex_never_carries_its_mcp_servers_credential_on_the_argv`,
`a_parked_request_with_no_tool_kind_reports_other`, the sweep's two edge tests.

Not covered by an automated test: the daemon runtime's call of `sweep_headless_dirs` at start-up, and
`subagent_end` still sending `keep_conversation: false` once `subagent_cancel` sends `true` (both
are green's wiring; `subagent_end` today calls `subagent_cancel_tool`, so green splits that path).

## Green wave

**Wave:** 1 of 1 (the sweep is the stack's last node).
**Greenable independently:** yes — every parent is green and pushed, and every test here drives this
node's own surfaces through the daemon's test service, the stub ACP agent and fake CLIs.
**Concurrent with:** none.
**Blocks:** none.
