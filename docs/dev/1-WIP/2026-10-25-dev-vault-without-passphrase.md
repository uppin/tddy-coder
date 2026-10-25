# Changeset: dev-vault-without-passphrase

## Problem

`./web-dev` and `./desktop-dev` sign in with stub logins, and their credential vaults prompted for a
passphrase like any other. After a restart that prompt could not be answered: choosing a first
passphrase needs a fresh sign-in on this daemon run (`SessionVaults::create` →
`VaultError::NoFreshLogin`), a session that only refreshed has none, and the daemon still reported
`UNINITIALIZED`, so the dashboard asked anyway and was refused —
*"choosing a credential vault passphrase needs a fresh GitHub sign-in on this daemon…"*.
`./desktop-dev` additionally kept no vault at all: `dev.desktop.yaml` set no `auth_storage`, so the
Accounts screen read *"Accounts are not enabled on this host"*.

## Decision (settled with the developer)

A development exclusion, fenced:

- **Explicit switch** `github.dev_vault_without_passphrase: true` — not inferred from a debug build.
- **Only with `stub: true`**: `tddy_daemon_auth::dev_vault::vaults_without_passphrase` refuses to
  start any other daemon, so no real GitHub token is ever sealed under a derivable key. The auth
  service also ignores the switch for a provider whose tokens are real (defence in depth). A
  startup warning says the vaults are as good as plaintext under `auth_storage`.
- **With it on**, `AuthServiceImpl` creates or opens a closed vault under `""` itself, at sign-in
  (`retain_the_login_credential`) and at every refresh (`reopen_the_vault`), and reports it OPEN —
  so `CredentialVaultPrompt` never shows. Creating at a refresh first notes a credential-less
  sign-in, exactly what a stub login notes, so `tddy-credentials`' fresh-sign-in invariant is
  unchanged: the library is not touched.
- **A vault created under a real passphrase** is never opened, reset or replaced: it stays LOCKED and
  prompts as before.
- Enabled in `dev.daemon.yaml` and `dev.desktop.yaml`; `dev.desktop.yaml` also gains
  `auth_storage: tmp/.tddy/auth` (cherry-picked from #765, identical there).

## Package docs to update at wrap

- `packages/tddy-daemon-kernel/docs/daemon-kernel.md` — the new `GitHubConfig` field, beside the two
  vault lifetimes.
- `packages/tddy-daemon-auth/docs/` — the `dev_vault` fence.
- `packages/tddy-github/docs/` — `with_vaults_without_passphrase` in the vault half of the auth
  service.

## Verification

`tddy-daemon-auth/tests/dev_vault_without_passphrase_acceptance.rs`: stub sign-in → OPEN with a
vault on disk; restart + refresh with no unlock key → OPEN; a session signed in before the vault
existed → refresh creates it; a real-passphrase vault stays LOCKED byte-for-byte; a real-GitHub
daemon with the switch does not start; both checkout dev configs keep vaults and pass the check.
