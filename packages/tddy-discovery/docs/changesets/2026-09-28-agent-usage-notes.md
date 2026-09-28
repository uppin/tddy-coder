# 2026-09-28 — A specialized agent def carries its operator's usage notes

**Type:** Feature

`#subagent-control` 3/5, [#555](https://github.com/uppin/tddy-coder/pull/555). Cross-package entry:
`docs/dev/changesets/2026-09-28-agent-usage-notes.md`.

`SpecializedAgentDef` gains an optional **`usage_notes`** — serde following the `api_key`
precedent (absent, not null, on the wire) and `Debug` redaction. It is the operator's
documentation of how to use the agent, read by humans: **nothing** in this crate's turn loop,
system-prompt assembly or spawn path consumes it. The agent-def YAML example in
`docs/ft/coder/specialized-subagents.md` shows the field.
