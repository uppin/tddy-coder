# 2026-09-23 — One rule decides who may be taken for a daemon

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`participant_identity.rs` (new): `may_be_daemon_discovery_identity` and `NON_DAEMON_IDENTITY_PREFIXES` (`web-`, `browser-`, `server`, `daemon-`, `split-agent-`, `remote-git-`, `screenshare-host-`), with `SPLIT_AGENT_IDENTITY_PREFIX` defined here. `token.TokenService` refuses every identity the rule allows, on every registration, and its refusal names every accepted prefix. `auth.proto`'s header no longer says the LiveKit secret signs session tokens.
