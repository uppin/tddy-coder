# 2026-07-11 — Subagent token accounting + `subagent_list`

**Type:** Feature

the subagent session table tracks agent name + turn count; `subagent_prompt` results carry a `usage` object; new `subagent_list` MCP tool returns every open conversation with its cumulative tokens; on each prompt/cancel the full list (`tddy_core::token_accounting::ConversationRecord` shape) is written to `TDDY_TOOLS_ACCOUNTING_FILE`. Feature [session-token-accounting.md](../../../../docs/ft/coder/session-token-accounting.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#289](https://github.com/uppin/tddy-coder/pull/289). (tddy-tools)
