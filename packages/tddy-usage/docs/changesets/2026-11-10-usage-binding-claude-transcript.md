# 2026-11-10 — tddy-usage: declarative usage bindings, Claude first

New crate. The binding model, path templates, field paths, the `jsonl` format, the recursive
conversation tree, `account` and `gather_session_usage`; `token_accounting` moved here from
`tddy-agent-backend` with `cacheReadTokens`, `cacheWriteTokens`, `reasoningTokens`, `costUsd?` and
`parentId?` (serde-defaulted), and `ConversationRecord::new`. `format_token_summary` prints the cache
and reasoning counters. `#usage-binding 1/5`, PR [#890](https://github.com/uppin/tddy-coder/pull/890).
See [usage-bindings.md](../usage-bindings.md).
