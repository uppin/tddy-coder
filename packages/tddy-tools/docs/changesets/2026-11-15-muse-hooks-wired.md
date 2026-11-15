# 2026-11-15 — tddy-tools: `request_user_input` is not a tool call

The `session-hook` tool-call exclusion covers Muse's `request_user_input` alongside `AskUserQuestion`: neither of its hooks leaves a pending call or an unpaired completed row. PR [#911](https://github.com/uppin/tddy-coder/pull/911). See [2026-11-15-muse-hooks-wired](../../../../docs/dev/changesets/2026-11-15-muse-hooks-wired.md).
