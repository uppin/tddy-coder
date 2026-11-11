# 2026-11-10 — Claude transcript reader removed; token accounting moved to tddy-usage

`backend/claude/transcript_usage.rs`, its `read_claude_*` re-exports and `backend::gather_session_usage`
are deleted; `token_accounting` re-exports `tddy_usage::token_accounting`. Usage now comes from the
agent spec's `coding_agent.usage` binding. `backend/mod.rs` 537 → 485 production lines, closing
`oversized-file-backend-mod`. `#usage-binding 1/5`, PR [#890](https://github.com/uppin/tddy-coder/pull/890).
