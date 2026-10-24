# Changeset: stub-login-gets-a-credential-vault

## Problem (State A)

A stub GitHub login (`github.stub: true`, as `dev.daemon.yaml` configures for `./web-dev`) was
kept out of the credential vault entirely: `AuthServiceImpl::retaining_vaults` filtered on
`issues_usable_access_token()`, so sign-in reported `VaultState::NONE`, `CredentialVaultPrompt`
never showed, and `UnlockVault` / `ResetVault` refused. `tddy-accounts` still read the same
`SessionVaults`, found none, and refused every provider-account RPC with "no credential vault
exists … choosing a passphrase creates one" — a passphrase nothing on the page could choose. The
Accounts screen told the operator to reload for a prompt that never came. Desktop, signed in to
real GitHub, prompted right after login.

## State B

- A stub login gets a vault like a real one: sign-in reports `UNINITIALIZED` / `LOCKED` / `OPEN`,
  the web prompt appears after login, and provider accounts can be connected on a local daemon.
- The stub's synthetic GitHub token is still **never retained** (D12 holds): PR lookups for stub
  mode are decided by `github.stub` (`pr_lookup_for_caller`), not by the vault, so they stay
  `PrLookup::Empty`.
- `tddy-credentials`: `SessionVaults::sign_in_without_credential(subject)` — over an open vault it
  hands the lineage an unlock slot and seals nothing; over a closed one it holds a credential-free
  pending mark (`Pending::record` is now `Option<CredentialRecord>`) that permits `create` /
  `reset` exactly like a waiting credential, for the same pending lifetime, and seals nothing.
- `tddy-github`: `retaining_vaults` → `vaults` (no stub filter); `retain_the_login_credential`
  calls `sign_in_without_credential` when the provider issues no usable token; `vaults_to_unlock`
  no longer refuses a stub.

Accepted relaxation: on a stub daemon, choosing the passphrase is gated by a stub sign-in, which
proves nothing about account control. Stub mode is local-only, so this is intended.

## Docs to update at wrap

- `packages/tddy-credentials/docs/credential-store.md` (~line 233, "A stub login retains nothing,
  creates nothing…") — a stub login now leaves a credential-free pending mark and can create a vault.
- `packages/tddy-daemon-auth/docs/auth-service.md` (~line 93, ~line 353) — "a stub provider retains
  nothing" → retains no token but has a vault; test table row now reads "a stub login reporting
  `UNINITIALIZED` and creating its vault".

## Tests

- `tddy-credentials` `sessions.rs`: `a_credential_free_sign_in_permits_creating_a_vault_that_holds_nothing`,
  `a_credential_free_sign_in_over_an_open_vault_gets_an_unlock_slot_and_seals_nothing`.
- `tddy-github` `github_token_retention_acceptance.rs`: `retains_no_github_token_for_a_stub_login`.
- `tddy-daemon-auth` `login_opens_the_credential_store_acceptance.rs`:
  `a_stub_login_is_asked_to_create_the_vault_and_writes_nothing_yet`,
  `a_stub_operator_creates_their_vault_with_a_first_passphrase`.
