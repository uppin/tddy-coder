# 2026-10-24 — The main agent can hand a subagent's permission request to the user

`#coagents` 8/8 (top), [#727](https://github.com/uppin/tddy-coder/pull/727), on #726.
**Packages:** tddy-service (proto + regenerated TS), tddy-discovery, tddy-coding-subagent, tddy-agent-backend, tddy-session-agents, tddy-session-lifecycle, tddy-session-split, tddy-github, tddy-tools, tddy-daemon-rpc (tests and test support), tddy-web.

- **Deferral.** `subagent_resume { deferPermissionToUser: "<requestId>" }` (`tddy-tools`
  `permission_deferral_arg`) → `TurnRequest::deferring_permission_to_user` → wire field
  `ResumeAgentConversationRequest.defer_permission_to_user` (11, `optional string`). The turn holds the
  request for the user and reports `{ pending: true, awaitingUser: true }` until it ends. Combined with
  `permission` it is refused naming both.
- **Answer route.** `SessionAgentService.AnswerAgentPermission` (request fields 1–7, response
  `answered_by`), peer-routed in `PeerRoutedSessionAgents`, not in-jail relayable. Authenticated, the
  agent's own token `PERMISSION_DENIED`, the conversation must be open on the named session, only a
  deferred request may be answered; a remote conversation's answer is forwarded to its owner
  (`AgentConversationPeers::answer_permission`) and the owner's answer or refusal returned as is.
  Delivery is through a handle captured at open (`SubagentSession::permission_answerer`,
  `AgentConversation::Local.answering`), so it does not wait on the deferred turn's lock.
- **First answer wins.** `PermissionAnswering` and `AnsweredBy` (in `tddy-discovery`, re-exported by
  `tddy-coding-subagent`) are the second resolver over `ParkedPermissions`. A main agent's answer is
  not queued behind its own deferred turn: delivered at once, its call refused `FAILED_PRECONDITION`
  saying so (`claim_for_answer`, and `run_turn`'s bypass in `tddy-discovery` for a remote
  conversation).
- **Records.** `AcpUpdate::ToolCallRecord` carries the request (pending, `deferredToUser`) and its
  answer (completed, `{answeredBy, optionId}` or `{answeredBy, cancelled: true}`) under
  `permission:<requestId>`. A daemon-written status row per deferred turn
  (`subagent-conversation:<id>`, `conversation_status.rs`) drives the `awaiting_user` fold in
  `subagent_conversation_replay`.
- **Cancel actor.** `CancelAgentConversation` records a pending request as answered by the caller the
  token names; `cancelled_by` (field 5) was added and then removed in this PR and is reserved.
  `SessionClaims.principal: Option<TokenPrincipal>` (`tddy-github`), minted by
  `SessionTokenSigner::mint_agent` for every agent credential (`mint_agent_session_token`); the
  daemon's `AgentSessions::answerer` reads it off a token its own keys verify.
- **Web.** `PermissionRequestEntry` / `TranscriptPermissionRequest` in `SubagentConversationDialog`,
  `useAnswerAgentPermission` addressed to `rosterHalfOf(session)`, "waiting for you" bubble label,
  "Answered by" / "Cancelled by"; the conversation tab's cancel sends no actor and is refused with no
  signed-in user (`callerLogin`).
- **Rebase onto the rebuilt #726.** Proto conflicts in `session_agents.proto` union-resolved
  (`AgentPlacement` beside the new messages) and TypeScript regenerated per commit; `service.rs`'s
  detach path now walks `close_every_conversation_with`'s `closed.local` / `closed.remote`;
  `ParkedPermissions::answer` restored as the main agent's answer, because #726's
  `cancel_request_targets_one_parked_request` calls it.
- **Test provenance.** `dbbc14952` (a cancel completes the status row) was written after the code it
  covers, not red-first. `coding_agent_session_acceptance::cancelling_a_running_turn_returns_cancelled`
  failed once in a full package run and passed in every repeat.
- **Backlog recorded:** [`2026-10-25-conversation-status-row-is-durable-only.md`](../todo/2026-10-25-conversation-status-row-is-durable-only.md)
  (the status row is not published live, and raw activity shows it as a tool call). The ⚠ DURING
  entry `2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it` stays: the answer
  route checks the session, the host-global lookup is unchanged.
- **Code issues** (deferred to the stack's sweep node by the developer, 2026-10-25): new
  `tddy-session-agents/…/oversized-file-conversation-recorder` (441 → 502),
  `tddy-web/…/oversized-file-sessions-drawer-screen` (497 → 501),
  `tddy-coding-subagent/…/complexity-session-take-turn` (39 → 71 lines, depth 5),
  `tddy-session-agents/…/complexity-service-cancel-agent-conversation` (49 → 74),
  `tddy-tools/…/complexity-server-subagent-resume-tool` (59 → 67); growth rows on
  `oversized-file-session` (575 → 699), `-service` (1,209 → 1,436), `-subagent-runtime` (745 → 823),
  `-subagent` (2,087 → 2,096), `-conversation` (551 → 560), `-server` (2,970 → 2,984),
  `complexity-service-take-a-turn` (122 → 148), `complexity-server-take-a-turn` (80 → 84). Under budget
  but grown: `subagent_conversation_replay.rs` 277 → 397, `session_token_v2.rs` 400 → 444 production
  lines (≈900 total, mostly tests), `tddy-session-lifecycle/src/test_util.rs` 367 → 448.

Feature docs: [agent activity pane § The user answers a deferred permission request](../../ft/web/agent-activity-pane.md#the-user-answers-a-deferred-permission-request-added-2026-10-25),
[managed-codebase subagents](../../ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer),
[session agent roster](../../ft/daemon/session-agent-roster.md#a-conversations-turn-control-crosses-the-wire).
