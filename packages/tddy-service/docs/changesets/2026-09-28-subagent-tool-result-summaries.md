# 2026-09-28 — AgentMessageDescriptor carries the tool result summary

**Type:** Feature

`#subagent-control` 1/5, [#553](https://github.com/uppin/tddy-coder/pull/553). Cross-package entry:
`docs/dev/changesets/2026-09-28-subagent-tool-result-summaries.md`.

`session_agents.proto`'s `AgentMessageDescriptor` gains `result_summary_json` (field 7) — the
subagent turn's per-tool result summary as one externally tagged JSON object, string-typed like
`stop_reason`. Empty for every non-tool role and for a dispatch that produced nothing.
