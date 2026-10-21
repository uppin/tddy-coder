# 2026-10-21 — Provider-account RPCs in `accounts.proto`

**Type:** Feature

`#provider-route` 1/9 ([#662](https://github.com/uppin/tddy-coder/pull/662)). `AccountsService`
gains `ConnectProviderAccount` (`session_token`, `preset`, `label`, inbound-only `api_key`,
`anthropic_base_url`, `openai_base_url`) and `ListProviderAccountModels` (`session_token`,
`account_id` → `model_ids`). `AccountSummary` gains `preset = 8` and `wire_protocols = 9` (additive).
Semantics: [tddy-accounts provider accounts](../../../tddy-accounts/docs/provider-accounts.md).
