# 2026-10-25 — `AuthServiceImpl::with_vaults_without_passphrase`

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md`](../../../../docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md)

`open_without_passphrase` creates or unlocks a closed vault under `""` at sign-in and refresh, noting a credential-less sign-in before a create; never for a provider with real tokens; a real-passphrase vault stays `LOCKED`.
