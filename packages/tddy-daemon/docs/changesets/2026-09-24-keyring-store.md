# 2026-09-24 — The runtime injects the credential vaults and starts the credential sweep

**Type:** Architecture

`#keyring` 3/9, PR [#510](https://github.com/uppin/tddy-coder/pull/510). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-store.md`](../../../../docs/dev/changesets/2026-09-24-keyring-store.md)

`runtime::build` hands `AuthBuildResult::credential_vaults` to the session host (`with_credential_vaults`) and starts `tddy_daemon_auth::vault_lifetimes::spawn_credential_sweep` beside it — pending sign-ins, and idle open vaults (named `pending_logins::spawn_pending_login_sweep` until the idle-vault follow-up, which left the line count unchanged) (+1 production line, 1,619 → 1,620; `build` 879 → 880 — `oversized-file-runtime` and `complexity-runtime-build` regressed, deferred with consent). Detail: [daemon-endpoint.md](../daemon-endpoint.md#assembling-the-session-host-and-the-rpc-families).
