# 2026-09-28 — session_agents.proto carries yield conditions

**Type:** Feature

`#subagent-control` 4/5, [#556](https://github.com/uppin/tddy-coder/pull/556). Cross-package entry:
`docs/dev/changesets/2026-09-28-turn-yield-conditions.md`.

`session_agents.proto`: `yield_conditions_json` on `PromptAgentConversationRequest` (field 7) and
`ResumeAgentConversationRequest` (field 8); `fired_condition_json` (field 6) on
`AgentConversationChunk`'s final frame.
