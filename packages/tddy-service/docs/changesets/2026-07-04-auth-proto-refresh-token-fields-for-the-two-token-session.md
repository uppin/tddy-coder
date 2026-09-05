# 2026-07-04 — `auth.proto`: refresh-token fields for the two-token session model

**Type:** Feature

`ExchangeCodeResponse` and `RefreshSessionResponse` gain `string refresh_token`; `RefreshSessionRequest`'s field 1 is renamed `session_token` → `refresh_token` (same field number, new semantics: it now carries a refresh-kind token, not an access token). Feature [session-auth.md § Durable sessions](../../../../docs/ft/daemon/session-auth.md#durable-sessions-access--refresh-tokens). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#283](https://github.com/uppin/tddy-coder/pull/283). (tddy-service, tddy-github, tddy-daemon, tddy-web)
