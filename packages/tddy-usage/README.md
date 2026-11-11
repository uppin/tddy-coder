# tddy-usage

Per-conversation token accounting that names no coding agent. An agent's spec declares where its
usage lives (`extensions.coding_agent.usage`); this crate evaluates that binding over the files the
agent writes and returns one `ConversationRecord` per conversation.

- `UsageSection` and friends — the serde binding model, validated by `validate_usage_section`.
- `account(section, ctx)` — the binding's rows: the main conversation, then its children recursively.
- `gather_session_usage(section, ctx)` — `account` plus `<session_dir>/egress/accounting.json`.
- `token_accounting` — `TokenUsage`, `ConversationRecord`, `format_token_summary`.

Details: [docs/usage-bindings.md](docs/usage-bindings.md). Feature:
[session-token-accounting.md](../../docs/ft/coder/session-token-accounting.md).
