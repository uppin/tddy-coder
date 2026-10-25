# 2026-10-25 — Close the `#coagents` stack's six functionality TODOs (sweep)

**Type:** Feature

`#coagents` 9/9 (top, sweep node), [#776](https://github.com/uppin/tddy-coder/pull/776), on #727.
**Packages:** tddy-service (proto + regenerated TS), tddy-discovery, tddy-coding-subagent,
tddy-agent-backend, tddy-session-agents, tddy-session-lifecycle, tddy-tools, tddy-daemon-rpc (tests),
tddy-daemon (start-up wiring).

## What landed

1. **A coding agent's `subagent_cancel` keeps the conversation.**
   `CancelAgentConversationRequest.keep_conversation` (6) and
   `CancelAgentConversationResponse.conversation_kept` (1). `SubagentSession::turn_canceller` /
   `TurnCancelling` (tddy-discovery), implemented by `CodingAgentSession` over its `CancelHandle` and
   forwarded by `CodingAgentConversationTurns`; captured as `AgentConversation::Local.cancelling` at
   open. The service dispatches on `OpenAgentConversations::turn_cancel_route` (`Local` →
   `cancel_local_turn`, `Remote` → `cancel_remote_turn`, `Close` for a model loop).
   `AgentConversationPeers::cancel` carries the flag and returns the owner's answer, and
   `PeerRoutedSessionAgents` returns it instead of a fixed empty response. A new prompt after a
   cancel-ended turn closes that turn into the history (`close_cancelled_turn`). `tddy-tools`:
   `subagent_cancel` asks without holding the conversation table and closes only on
   `conversation_kept: false`; `subagent_end` calls the new `subagent_close_tool`. The
   `TODO(acp-subagent n3)` marker is gone.
2. **`CancelAgentConversation` checks the session.** `remove_on` / `turn_cancel_route` take the
   session id; a conversation open on another session is `NOT_FOUND`, worded as an unknown id.
3. **`PermissionRequest.kind`** (tddy-agent-backend), filled from `tool_call.fields.kind` in
   `mapping::permission_request`; `view_of` passes it through, `other` when `None`. The
   `FIXME(permission-yield)` is gone.
4. **No residue from a failed headless open.** The plan binds the approval route last;
   `ClaimedFiles` removes a directory a failing plan created; the service's `OpeningConversation`
   guard runs `conversation_closed` when an open fails after the plan (or its caller goes away), and
   is not armed for an id already open. Conversation directories are tagged with the daemon's
   durable instance id (`daemon-instance`, `claim_headless_conversation_dir`), and
   `sweep_headless_dirs(tddy_data_dir, instance_id)` — via
   `sweep_headless_files_left_by_this_daemon`, called by the daemon runtime's `build_session_host`
   before it serves — removes only this daemon's. *(Developer decision during green: the first
   draft swept the whole of `headless/`.)*
5. **The status row is live and marked.** `CONVERSATION_ROW_SOURCE = "subagent-status"`;
   `conversation_status::record` publishes on `SessionAgentPorts.activity` (the `AgentActivityHub`)
   after appending; `conversation_row_entry` keys on the source. The `TODO(web-approvals)` is gone.
6. **Headless cursor, oz and codex get tddy-tools MCP.** cursor: `<worktree>/.cursor/mcp.json`,
   `0600`, added to `info/exclude` first, an existing file refused; oz: `--mcp <path>` with top-level
   servers; codex: `HeadlessCommand.mcp_server` → `-c mcp_servers.<name>.{command,args,env_vars}`,
   values in the CLI's environment only. `confinement_for` is unchanged.

## Found and fixed at validation (#776, after green)

- **The advertised `subagent_cancel` said "Close an open subagent session"**, and `subagent_end`'s
  texts told the main agent to use `subagent_cancel` to discard work — both untrue for a coding
  agent after item 1. Rewritten; the definition is now `subagent_cancel_tool_definition()`, pinned by
  `subagent_end::tests::subagent_cancel_says_it_keeps_a_coding_agents_conversation_rather_than_closing_every_one`.
- **Two doc comments were attached to the wrong item** (green inserted `close_cancelled_turn` and
  `OpeningConversation` between an existing doc comment and its function, so `finish`'s and
  `record_cancelled_request`'s docs described the new items). Moved back.

## Backlog

Closed (✅ RESOLVED HERE, entries deleted):

- *Coding-agent cancel does not reach `session/cancel`* (2026-10-24) — item 1.
- *`CancelAgentConversation` does not check the session* (2026-10-25) — item 2.
- *The ACP permission request drops the tool kind* (2026-10-24) — item 3.
- *Headless approval-route residuals* (2026-10-25) — item 4.
- *The conversation status row is durable only* (2026-10-25) — item 5.

Narrowed: *Headless cursor, codex and oz get no MCP config* (2026-10-25) → renamed
`2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md`, now only the withdrawal-mode
half. *A conversation id is not bound to the session that opened it* (2026-09-26) — cancel now
checks; prompt and resume still do not.

Opened:

- `2026-10-25-cursor-project-mcp-config-outlives-its-conversation.md`
- `2026-10-25-headless-sweep-cannot-tell-two-hostname-daemons-apart.md`
- `2026-10-25-start-up-headless-sweep-call-is-untested.md`
- `2026-10-25-no-tool-discards-a-coding-agents-conversation.md` (found at validation: with item 1,
  no tool ends a coding agent's conversation without applying its work — needs a surface decision)

## Code issues

Deferred by the developer for the whole stack, past this node too (2026-10-25): recorded, not split.
New: `tddy-session-lifecycle/…/complexity-coding-agent-launcher-headless-plan` (59 → 91 lines).
Growth rows: `oversized-file-service` (1,436 → 1,604), `-coding-agent-launcher` (695 → 872),
`-session` (699 → 731), `-server` (2,984 → 3,036), `-conversation` (560 → 580), `-subagent`
(2,096 → 2,113), `-conversation-recorder` (502 → 512); `complexity-service-cancel-agent-conversation`
(74 → 111), `complexity-service-open-agent-conversation` (107 → 115),
`complexity-session-take-turn` (71 → 78).

## Verification

Scoped, local (2026-10-25, on `ff19a0863` + this PR, with the validation fixes):

- `./test -p tddy-agent-backend -p tddy-coding-subagent -p tddy-daemon -p tddy-daemon-rpc -p tddy-discovery -p tddy-service -p tddy-session-agents -p tddy-session-lifecycle -p tddy-tools`
  — 2,640 passed, 16 failed, 3 ignored. All 16 failures are the known macOS harness gap in four
  `tddy-session-lifecycle` sandbox suites (`sandbox_behavior_acceptance`, `sandboxed_claude_cli_acceptance`,
  `sandboxed_cursor_cli_acceptance`, `sandboxed_session_lifecycle_acceptance`: "sandbox RPC bridge not
  installed"), not regressions. `tddy-remote-git-repo-cli` was built first.
- `cargo clippy -p <those 9 packages> --all-targets -- -D warnings` clean; `cargo fmt --check` clean.
- Web: only the regenerated `session_agents_pb.ts` changed; no spec touched, none run.
- Not covered by a test: the runtime's start-up call of the sweep (backlog entry above).

Feature docs: [Session agent roster § Coding agents](../../ft/daemon/session-agent-roster.md#coding-agents),
[managed-codebase subagents](../../ft/coder/managed-codebase-subagents.md),
[agent activity pane § Transcript and status](../../ft/web/agent-activity-pane.md#transcript-and-status).
