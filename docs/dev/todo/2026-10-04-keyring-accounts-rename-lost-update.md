# 2026-10-04 — renaming an account can overwrite a concurrent write to the same record

**Category:** Deferred from `#keyring` 4/9 `accounts` (#511)
**Source:** /pr-wrap step 1 on #511

`SessionVaultAccountStore::set_label` in `packages/tddy-accounts/src/vault_store.rs` reads the record
with `SessionVault::get`, then writes it back with `SessionVault::put`. Each call is serialised on its
own, but the pair is not: a write to the same record landing between them — a link flow refreshing
the secret, say — is overwritten with the secret read earlier.

**Why deferred:** closing it needs a read-modify-write (or compare-and-set) operation on
`SessionVault`, which is `tddy-credentials`' surface (`#keyring` 3/9, #510, merged). By its boundary
rules this node adds nothing to that crate.

## What would close it

Add an atomic update to `tddy-credentials` — e.g.
`SessionVault::update(provider, account, FnOnce(&mut CredentialRecord))` — and have `set_label` use
it instead of `get` + `put`. Delete this entry then, along with the `TODO(keyring)` in `set_label`.

## A second site: setting a provider default (#666, `#provider-route` 5/9)

`SessionVaultAccountStore::set_provider_account_defaults` (same file) lists the vault, then `put`s
up to two records (the new holder and the one that lost the harness). It holds the store's
`new_provider_accounts` lock, so it is serialised against a connect and against another default
change, but not against `set_label` or a sync writing the same record between the list and the put.
It carries the same `TODO(keyring)`. The `SessionVault::update` named above closes both; when it
lands, have the default change use it per record and remove that marker too.
