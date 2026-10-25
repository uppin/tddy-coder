# 2026-10-24 — A coding subagent's permission request is the main agent's to answer

`#coagents` 3/8, [#722](https://github.com/uppin/tddy-coder/pull/722), on #721.
**Packages:** tddy-discovery, tddy-coding-subagent, tddy-session-agents, tddy-service (proto), tddy-tools, tddy-web (regenerated TS).

- A coding subagent's ACP `session/request_permission` parks in `ParkedPermissions`; the tddy turn ends
  `permission_requested` with the request, while the ACP prompt stays in flight. `subagent_resume
  { permission }` resolves it and the same prompt continues.
- Proto: `AgentConversationChunk.permission_request_json` (8), `ResumeAgentConversationRequest.permission_answer_json` (10).
- #721's `a_permission_request_is_denied_and_recorded_in_the_turns_messages` was rewritten as
  `a_permission_request_yields_instead_of_being_auto_answered` (developer's decision); after the
  rebase onto #721's tip, its `cancelling_a_running_turn_returns_cancelled` answers the yielded
  request before cancelling, since a turn no longer runs past a permission request on its own.
- Decisions: always yield (no auto-allow); cancel waits for the turn, bounded at 10 s; a model loop
  refuses a permission answer.
- Code issues: `tddy-coding-subagent/src/session.rs` crossed the budget (323 → 501, new record);
  `subagent.rs` 2,058 → 2,078, `subagent_runtime.rs` 735 → 740, `roster/conversation.rs` 526 → 551,
  `service.rs` 1,149 → 1,155, `server.rs` +6 — all stack-shared, split after the stack lands.
- Deferred: [`docs/dev/todo/2026-10-24-acp-permission-request-drops-tool-kind.md`](../todo/2026-10-24-acp-permission-request-drops-tool-kind.md) — the tool kind is always `other` until #720's driver passes it through.

Feature doc: [managed-codebase subagents § permission requests](../../ft/coder/managed-codebase-subagents.md#a-coding-subagents-permission-request-is-the-callers-to-answer).
