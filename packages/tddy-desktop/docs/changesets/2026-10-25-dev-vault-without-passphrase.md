# 2026-10-25 — `./desktop-dev` keeps credential vaults, and opens them without a passphrase

**Type:** Fix

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md`](../../../../docs/dev/changesets/2026-10-25-dev-vault-without-passphrase.md)

`dev.desktop.yaml` sets `auth_storage: tmp/.tddy/auth` and `github.dev_vault_without_passphrase: true`; `config_source`'s test pins `auth_storage`. Detail: [config-resolution-and-install.md](../config-resolution-and-install.md).
