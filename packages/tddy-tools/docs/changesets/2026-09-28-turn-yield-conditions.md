# 2026-09-28 — subagent_prompt and subagent_resume accept yieldConditions

**Type:** Feature

`#subagent-control` 4/5, [#556](https://github.com/uppin/tddy-coder/pull/556). Cross-package entry:
`docs/dev/changesets/2026-09-28-turn-yield-conditions.md`.

The `subagent_prompt` and `subagent_resume` MCP schemas gain **`yieldConditions`** — per-turn
conditions on a tool call (an outcome fact from the result summary's vocabulary, or an argument
string field containing a bounded substring), parsed and validated at the server before the
request reaches the daemon.
