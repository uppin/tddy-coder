# 2026-10-24 — A subagent conversation is one activity bubble that opens it

`#coagents` 4/8, [#723](https://github.com/uppin/tddy-coder/pull/723), on #722.
**Packages:** tddy-service (proto), tddy-session-agents, tddy-session-activity, tddy-session-lifecycle,
tddy-coding-subagent, tddy-discovery, tddy-daemon-kernel, tddy-coder, tddy-web (+ regenerated TS).

- Daemon-run conversations record a per-conversation ACP transcript
  (`<session>/conversations/<id>/acp-transcript.jsonl`): streamed coding-agent updates as they arrive
  (`ConversationRecorder` on `SubagentSession::updates()`), model loops whole at turn end. A turn
  that cannot be recorded fails.
- `conversation_id` on `StreamAcpReplayRequest` (6), `GetAcpReplayPageRequest` (6),
  `GetAcpToolCallDetailRequest` (5); a conversation stream tails `ConversationFrames`
  (`tddy-daemon-kernel`). `tddy-coder` refuses conversation replay with `NOT_FOUND`.
- `frame_for_agent_activity` folds `subagent_*` hook records into one `subagent:<id>` tool call titled
  `<agent> · <first prompt>`; the web renders it as `from: "subagent"` and opens
  `SubagentConversationDialog`.
- Changed from the plan: the replay-RPC suite lives in `tddy-session-lifecycle`'s unit tests; the
  `recorded` flag was dropped (the replay's `NOT_FOUND` tells an unrecorded conversation apart); test
  11 uses the exclusive `before_seq: 1`.
- Accepted risks: live seq = append count (diverges only on a repeated `tool_call` id); a recorder
  1024 updates behind fails the turn; the transcript write holds the conversation's lock; a bubble
  first seen live keeps its first title until reload.
- Code issues (stack-shared, split after the stack lands): coding-subagent `session.rs` 501 → 530,
  discovery `subagent.rs` 2,078 → 2,087, session-agents `service.rs` 1,155 → 1,191.
- Backlog: `2026-08-29-a-roster-agent-s-own-turns-are-unobservable-from-the-web.md` narrowed to the
  in-jail remainder.

Feature doc: [agent activity pane § a subagent conversation is one bubble](../../ft/web/agent-activity-pane.md#a-subagent-conversation-is-one-bubble-added-2026-10-24).
