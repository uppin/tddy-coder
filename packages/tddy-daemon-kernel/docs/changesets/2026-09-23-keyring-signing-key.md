# 2026-09-23 — Two doc comments, and the split-agent prefix re-exported

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`config.rs`: `auth_storage` names the signing key; `LiveKitConfig::enabled` no longer claims `api_secret` signs session tokens. `daemon_identity` re-exports `SPLIT_AGENT_IDENTITY_PREFIX` from `tddy_service::participant_identity`. Detail: [daemon-kernel.md](../daemon-kernel.md).
