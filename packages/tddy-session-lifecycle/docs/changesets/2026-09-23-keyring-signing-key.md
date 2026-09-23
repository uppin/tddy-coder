# 2026-09-23 — Split and jailed-codebase agents are credentialed through `SessionTokens`

**Type:** Architecture · `#keyring` 1/9, PR [#508](https://github.com/uppin/tddy-coder/pull/508)
Cross-package entry: [`docs/dev/changesets/2026-09-23-keyring-signing-key.md`](../../../../docs/dev/changesets/2026-09-23-keyring-signing-key.md)

`DaemonSessionHost::with_session_tokens`; `split_session.rs` verifies callers and mints agents' tokens with the daemon's own key instead of `livekit.api_secret`, and `split_remote_tool_env` takes a `SplitSpawnTarget`. The exec-tool refusal tells an operator the codebase host has not learned the agent host's key. Suites migrated off the shared secret; `session_room_acceptance` names its lobby per fixture.
