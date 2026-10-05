# 2026-10-05 — credential sync only runs while exactly one subject is signed in

**Category:** Deferred scope from `#keyring` 6/9 `credential-sync` wiring
**Source:** `runtime::build`'s construction of `tddy_credential_sync::SyncEngine`
(`packages/tddy-daemon/src/credential_sync.rs`)

`tddy_credential_sync`'s wire format (`WrappedRecords`, and the `VaultEntry`s it carries) names no
subject — a `(provider, account)` record is addressed to a *daemon*, never to a *person* on that
daemon. `tddy_daemon_auth::SessionVaults` can legitimately hold more than one open vault at once (a
server daemon with several signed-in operators), and nothing on the wire would let a receiving
daemon route an incoming record to the right one of them — two different people's `github/jane-doe`
entries would collide in one subject's vault.

**Why deferred:** closing it needs a subject field added to the wire types
(`WrappedRecords`/`PeerAdvertisement` or an envelope around them), which changes `tddy-credential-sync`'s
already-shipped, tested wire format — out of this node's boundary (`#keyring` 6/9 was told not to
change that crate's public API without stopping to ask first).

## What stands today

`credential_sync::publish_if_exactly_one_vault_is_open` (`packages/tddy-daemon/src/credential_sync.rs`)
checks how many of `config.users`' enrolled subjects have an open vault
(`tddy_daemon_auth::SessionVaults::state`) immediately before every publish attempt:

- **Zero** open vaults: nobody is signed in, nothing to sync — the common case between logins.
- **Exactly one**: that subject's vault is published to this daemon's admitted peers.
- **More than one**: the attempt is skipped and logged at `info` (not a warning — several signed-in
  operators on one server is a normal, supported deployment shape this feature does not yet cover,
  not a fault). Sync resumes on its own once only one vault is open again.

`SessionVaults` itself has no public enumeration of which subjects are currently open (by design —
see its own module doc on why a lookup must never block), so the check above goes by the population
`config.users` names and asks `state()` of each, rather than a count the vault registry keeps.

## The second gap this uncovered

`tddy_credential_sync`'s own `SyncEngine::receive` has nowhere to write a received record back to a
vault that already fits `SessionVault::put`'s shape for a `VaultEntry::Record`, but **no path exists
to retain a received `VaultEntry::Tombstone` with the version and `deleted_at` the sender attached**:
`SessionVault::remove` only ever mints its own tombstone from its own clock and its own last-seen
version, discarding whatever a peer sent. Last-writer-wins reconciliation (already decided correctly
by `SyncEngine::receive`) would then be undone by whichever daemon's tombstone has the newer local
clock reading, not the one the two peers actually agreed resolved the deletion.

Nothing in this node registers a receiving-side RPC handler for `#keyring` 6/9's
`CredentialSync.Send` either (see `LiveKitPeerTransport`'s own `CREDENTIAL_SYNC_SERVICE` doc
comment) — `publish` reaches a peer that answers "unknown method" today, and is journaled
`Undeliverable`. Wiring that handler in surfaces the tombstone gap immediately, so the two should be
closed together.

## What would close both

1. A subject field on the wire (above), so a multi-vault daemon can route an incoming record.
2. A retain-with-given-version operation on `tddy-credentials`' `SessionVault` — the same shape
   `2026-10-04-keyring-accounts-rename-lost-update.md` already asks for a read-modify-write there,
   so the two may be worth doing in the same pass.
3. The receiving-side RPC registration itself, once 1 and 2 exist to receive into.

Delete this entry once all three land and credential sync runs end to end between two real daemons.
