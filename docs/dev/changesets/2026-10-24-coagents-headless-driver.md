# 2026-10-24 — Headless `-p` coding agents as session subagents

`#coagents` 7/8, [#726](https://github.com/uppin/tddy-coder/pull/726), on #725.
**Packages:** tddy-agent-backend, tddy-coding-subagent, tddy-session-lifecycle, tddy-session-agents, tddy-daemon-rpc, tddy-daemon, tddy-session-tool-client, tddy-tools, tddy-tool-engine, tddy-discovery, tddy-session-split, tddy-service (proto), tddy-web (picker + regenerated TS), tddy-toolcall (test).

- `HeadlessDriver` runs `claude -p`, `cursor agent -p`, `codex exec --json` and Warp's `oz agent run`
  once per turn behind `ConversationDriver`, the trait `CodingAgentSession` now holds (ACP via
  `AcpConversation`). Streams map to the ACP updates; cancel kills the process group.
- Catalog: headless harness **profiles** are offered as coding agents; every coding-agent row carries
  `transport` (`acp` / `headless`) and `permission_policy` (`asks_caller` / `fixed: …`), derived from
  the harness id (`transports.rs`), so `CodingAgentDef` is unchanged. Proto: `SubagentInfo` 10/11,
  `SessionAgentEntry` 16/17.
- Approvals: a headless `claude`'s `--permission-prompt-tool` is tddy-tools' `approval_prompt`, which
  calls the new `ExecToolService.RequestApproval` on the tool route. `ApprovalRoutes` binds each
  conversation at launch to its session and minted credential, parks the request on the session's
  `ParkedPermissions` (the `#coagents` 3/8 yield), and denies after 60 s, on cancel, or when the
  client drops. `ExecToolRpcHandler` field budget 9 → 10 (developer-approved).
- Decisions: offering from profiles only; tagged approvals skip local pre-allow (only
  `mcp__tddy-tools__*`); ownership bound at launch rather than in the token; the earlier
  `TDDY_SOCKET` relay and an unbound listener-side `ConversationApprovalHandler` were built and then
  removed in this PR (developer-approved deletion of their five tests). A new
  `tddy-toolcall` regression test pins that an untagged approve still reaches the presenter.
- Known limits (developer-decided): clone placement only; the daemon's own OS user only; no headless
  CLI is confinable, so managed sessions refuse them; `cursor`/`codex`/`oz` get no MCP config; cursor
  tool calls are not surfaced and codex's arrive at turn end; proven against fake CLIs only (oz
  fixtures are real captures).
- Rebase onto the rebuilt #725: `AgentPlacement` now from `session_agents.proto` (union-resolved with
  the new fields, TS regenerated); two `ExecToolService` test doubles gained `request_approval`. The
  three reds previously attributed to #725's stubs (`workspace_roster_admission`,
  `daemon_http_rpc_acceptance`, `remote_start_session_passes_each_agent`) pass.
- Backlog recorded: [`2026-10-25-headless-cursor-codex-oz-get-no-mcp-config.md`](../todo/2026-10-25-headless-cursor-codex-oz-get-no-mcp-config.md),
  [`2026-10-25-headless-approval-route-residuals.md`](../todo/2026-10-25-headless-approval-route-residuals.md),
  [`2026-10-25-cancel-agent-conversation-does-not-check-the-session.md`](../todo/2026-10-25-cancel-agent-conversation-does-not-check-the-session.md) (pre-existing, widened),
  [`2026-10-25-tddy-tools-spawn-conversation-test-races-the-socket-env.md`](../todo/2026-10-25-tddy-tools-spawn-conversation-test-races-the-socket-env.md) (pre-existing).
- Code issues (deferred to the stack's sweep node by the developer, 2026-10-25): new
  `tddy-session-lifecycle/…/oversized-file-coding-agent-launcher` (419 → 685),
  `tddy-agent-backend/…/complexity-headless-driver-run-turn` (80 lines),
  `tddy-session-tool-client/…/complexity-approval-request-approval` (104 lines); growth rows on
  `oversized-file-session` (530 → 575), `-backend-mod` (850 → 851), `-registry` (806 → 808),
  `-service` (1,203 → 1,209), `-connection-service` (+3); `tddy-tool-engine` `-lib` unchanged (877).

Feature docs: [session agent roster § Coding agents](../../ft/daemon/session-agent-roster.md#coding-agents),
[managed-codebase subagents § permission requests](../../ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer).
