# 2026-10-21 — Per-session provider-account record

**Type:** Feature

`#provider-route` 2/9 ([#663](https://github.com/uppin/tddy-coder/pull/663)). New `session_provider_account` module: `provider-account.yaml` beside `.session.yaml`, holding
`SessionProviderAccount { account_id, model }` and never a key;
`write_session_provider_account` / `read_session_provider_account` (`Ok(None)` when absent). See
[architecture](../architecture.md#session-provider-account-session_provider_account).
