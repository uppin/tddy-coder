# 2026-09-28 — The agent conversation wire carries yield conditions and the fired one back

**Type:** Feature

`#subagent-control` 4/5, [#556](https://github.com/uppin/tddy-coder/pull/556). Cross-package entry:
`docs/dev/changesets/2026-09-28-turn-yield-conditions.md`.

`PromptAgentConversationRequest` and `ResumeAgentConversationRequest` gain
`yield_conditions_json` (string-typed JSON, like the other per-call controls); the final
`AgentConversationChunk` gains `fired_condition_json`, present only on a `yieldedToCaller`
outcome. An unparseable conditions payload is refused with `Status::invalid_argument` **before any
turn is stamped** — never silently read as no conditions.

Code issue at wrap: `docs/code-issues/oversized-file-service.md` — 1,122 production lines (was
1,076), +46. Open, unclaimed; restructuring deferred past the `subagent-control` stack.
