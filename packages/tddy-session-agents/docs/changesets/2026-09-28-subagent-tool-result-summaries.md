# 2026-09-28 — The agent-conversation final frame carries each tool result's structured facts

**Type:** Feature

`#subagent-control` 1/5, [#553](https://github.com/uppin/tddy-coder/pull/553). Cross-package entry:
`docs/dev/changesets/2026-09-28-subagent-tool-result-summaries.md`.

`AgentMessageDescriptor` gains **`result_summary_json`** (field 7): the subagent turn's per-tool
result summary as one externally tagged JSON object, string-typed like `stop_reason` so the wire
stays additive and a caller reads the same shape the MCP turn outcome carries. The server-side
`message_descriptor` maps it from the descriptor; the client-side `parse_message_descriptor`
parses it back, refusing an unparseable payload rather than reporting a silent absence. Every
summary is bounded (~600 bytes worst case per tool-role descriptor), recorded in the accounting of
`docs/dev/todo/2026-09-26-a-turns-message-list-can-overflow-the-chunk-framing-threshold.md` — the
frame-budget defect itself stays open.

Code issue at wrap: `docs/code-issues/oversized-file-service.md` — 1,076 production lines (was
1,069), +7, one field mapping. Open, unclaimed; restructuring deferred past the
`subagent-control` stack.
