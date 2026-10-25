# 2026-10-25 — Dev daemons open their credential vaults without a passphrase

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Packages: `tddy-daemon-kernel`, `tddy-daemon-auth`, `tddy-github`, `tddy-desktop`; `dev.daemon.yaml`,
`dev.desktop.yaml`.

**Problem.** `./web-dev` and `./desktop-dev` sign in with stub logins, and their vaults prompted like
any other. After a restart that prompt could not be answered — a first passphrase needs a fresh
sign-in on the running daemon (`NoFreshLogin`), a refreshed session has none, yet the daemon reported
`UNINITIALIZED`. `./desktop-dev` kept no vault at all (no `auth_storage`), so the Accounts screen read
"Accounts are not enabled on this host".

**Change.**

- `dev.desktop.yaml` keeps vaults: `auth_storage: tmp/.tddy/auth`.
- `github.dev_vault_without_passphrase: true` — `AuthServiceImpl` creates or opens a closed vault
  under `""` at sign-in and at every refresh and reports it `OPEN`; at a refresh it first notes a
  credential-less sign-in, as a stub login does, so `tddy-credentials` is unchanged. A
  real-passphrase vault stays `LOCKED`. Refused at startup unless `stub: true`
  (`tddy_daemon_auth::dev_vault`), ignored for a provider with real tokens, warned about at startup.
  On in both dev configs.

Detail: [`tddy-daemon-auth` auth-service.md § Development vaults without a passphrase](../../../packages/tddy-daemon-auth/docs/auth-service.md#development-vaults-without-a-passphrase),
[session-auth.md § Security / configuration](../../ft/daemon/session-auth.md#security--configuration).

**Code issue measured at wrap (still open, not split here):** `tddy-daemon-auth` `auth.rs` 569 → 579
production lines (budget 500); recorded in its code-issue file.
