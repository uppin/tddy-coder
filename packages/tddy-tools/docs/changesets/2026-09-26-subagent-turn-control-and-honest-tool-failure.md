# 2026-09-26 — `subagent_resume`, `maxTurns`, and a turn outcome that lists what it appended

**Type:** Feature

`subagent_prompt` accepts `maxTurns`: how many model turns *that one call* may spend, in place of
the agent definition's budget. A value outside `1..=50` is clamped to the nearer bound rather than
refused, and the outcome carries `clampedMaxTurns` — a caller that asks for too much keeps working
and still learns what it got. A malformed value is refused naming the field, never ignored.

New tool **`subagent_resume`** `{sessionId, fromMessageId?, correction?, maxTurns?, graceMs?}`
takes another turn without asking anything new. Present-but-empty is refused for both reshaping
fields, for the reason `subagent_prompt` refuses an empty prompt. It queues, defers with a
`responseId` and reports `queuePosition`/`queueSize` exactly as a prompt does.

Every turn outcome — prompt, await and resume alike — carries `messages`, the messages that turn
appended, each with an id, a role, the tool, its `tool_calls`, `is_error` and a truncated preview.

`tests/mcp_tool_advertisement_audit.rs` moves 43/40 → **44/41**, pinned by name over the real
`--mcp` stdio wire, and `tests/subagent_resume_mcp_acceptance.rs` drives the new tool over the same
wire: advertisement and gating, the message list and its preview bound, the clamp, and an unknown
conversation as an in-band error result.

[README.md](../../README.md) § The MCP surface ·
[docs/ft/coder/managed-codebase-subagents.md](../../../../docs/ft/coder/managed-codebase-subagents.md)
§ Turn control · [../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
