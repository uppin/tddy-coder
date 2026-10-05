# Credential sync

`tddy-credential-sync` propagates `tddy-credentials`' records to the peers authorised to hold them —
wrapped per recipient, admitted by two independent checks, reconciled last-writer-wins with
tombstones, and journaled on both sides so every daemon knows where it stands.

It depends on `tddy-credentials` and nothing else in this workspace — not `tddy-daemon-livekit`, not
`tddy-daemon-auth`. `PeerTransport` and `IdentityVerifier` are the two ports that keep it that way:
`tddy-daemon-livekit` implements the first (`LiveKitPeerTransport`), and `tddy-daemon` bridges the
second to `#keyring` 1/9's `KeyDirectory`. An engine that reached for LiveKit directly would repeat
`heavy-dependency-livekit-peer-forwarding` exactly — one misplaced dependency, several crates paying
for something they never use — and would be untestable for its one interesting property: what a peer
failing exactly one check does *not* receive. A real LiveKit room cannot be made to fail one check at
a time; an in-memory fake can.

## Room membership is not the gate

`docs/ft/daemon/livekit-peer-discovery.md` § *Trust model* states the property this crate exists to
change: any participant in the common room may receive a forwarded session start, and there is no
cryptographic attestation that a participant runs `tddy-daemon` at all. That is a tolerable property
for forwarding a session start to a host an operator picked from a list. It is not tolerable for
handing out a person's GitHub credential — anything holding the room's LiveKit credentials would
receive it. `ListEligibleDaemons` and `StartSession` forwarding keep exactly the trust model they had;
this crate adds a gate **for credentials only**, and does not retrofit one onto forwarding.

## Admission: two checks, both required

```rust
pub struct SyncEngine { /* … */ }
pub trait PeerTransport: Send + Sync {
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError>;
    fn peers(&self) -> Vec<SignedAdvertisement>;
    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError>;
}
pub trait IdentityVerifier: Send + Sync {
    fn verify(&self, signing_key_id: &str, message: &[u8], signature: &[u8])
        -> Result<bool, RefusalReason>;
}
```

A peer advertises a `PeerAdvertisement { peer, signing_key_id, transport_public_key, challenge,
group_proof }`, signed into a `SignedAdvertisement`. `SyncEngine::admit` checks, in order:

1. **The Ed25519 signature**, through `IdentityVerifier` — *did that daemon send this?* A key id the
   directory has never published is `RefusalReason::UnknownIdentity`; a key it knows, under a
   signature that does not verify, is `SignatureInvalid`.
2. **The group proof**, `GroupSecret::verifies(challenge, group_proof)` — HMAC-SHA256 under this
   deployment's configured `keyring.group_secret`, compared in constant time. A mismatch, or no
   `group_secret` configured at all, is `GroupSecretMismatch`.

**Neither check admits a peer alone**, and that is the property worth stating twice: the group
secret without the signature lets anyone who captured one advertisement replay it; the signature
without the group secret authenticates a daemon nobody in this deployment authorised to hold its
credentials. No third path exists, and no "it's already in the room" shortcut re-derives
authorisation from membership — that re-derivation is exactly the thing being removed.

A daemon with no `keyring.group_secret` configured — the common case for a desktop install with no
fleet — syncs with **nobody**, reported as `SyncError::NotConfigured` rather than silently treated as
"sync with everyone".

## The wire: what crosses it, and what never does

```rust
pub struct VaultTransportKey { /* X25519, signed by the identity key, via KeyDirectory */ }
pub struct WrappedRecords {
    pub recipient: PeerId,
    pub sender_transport_public_key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}
```

Every daemon holds an X25519 `VaultTransportKey`, persisted beside its Ed25519 identity key and
advertised (its public half) signed by that identity — substituting a transport key is therefore a
signature failure, not a silent downgrade. A payload is sealed under a secret the sender derives from
its own secret half and the recipient's advertised public half (HKDF over the agreement, then
ChaCha20-Poly1305), with the recipient and the sender's public key as associated data — a payload
cannot be replayed as addressed to, or sent by, somebody else.

**Two daemons that sync end up sharing a *record*, never a *key*.** The payload carries a
`CredentialRecord`'s or a `Tombstone`'s fields (through a private wire mirror, the same pattern
`tddy-credentials`' own sealing uses — neither type derives `Serialize` for the secret-leak reason
`docs/credential-store.md` states). It never carries the vault's data key, its KEK, or the login
credential that seeded it. The recipient re-seals what it receives under **its own** vault key before
anything touches its disk, so compromising one daemon does not unwrap the other's.

## Reconciliation: last-writer-wins, with tombstones as answers

```rust
pub enum Reconciliation { TakeIncoming { conflicted: bool }, KeepLocal { conflicted: bool }, AlreadyConverged }
```

`SyncEngine::reconcile(local, incoming)` is pure and associated rather than a method, because both
daemons must compute it identically — a reconciliation that consulted engine state would converge
differently on the two sides, which is the one thing this must not do. The later `written_at()` wins
(a record's `updated_at`, a tombstone's `deleted_at` — one clock both variants share). **A tombstone
is an answer, not an absence**: it competes on its clock exactly as a record does, which is what
stops a deleted account resurrecting from a peer that still holds it, and equally what lets a
genuinely later re-link win back.

A conflict — both sides edited independently — is flagged (`conflicted: true`) for **two `Record`s
sharing a version**, not for a tombstone sharing its record's version: a tombstone's version is *the
version it deletes*, by design, so a tombstone and an unedited copy of that same version is an
ordinary last-writer-wins race on the clock, not two edits. Equal versions and equal clocks (not
exercised by any fixture) break the tie on version, the later one winning.

## The journal, and the Accounts-screen badge

```rust
pub enum SyncStatus { Pending, Sent, Acknowledged, Refused(RefusalReason), Conflict, Undeliverable }
pub enum AccountSyncSummary { Synced, Pending, Undeliverable, Conflict, Refused }  // worst first
```

`SyncJournal` holds one line per `(peer, record)` — the latest, not the history, since a screen
asking "where does this account stand with that peer" wants the answer, not every prior status.
`Refused` and `Undeliverable` are kept distinct: a refusal is the peer group working as configured
and the remedy is configuration; undeliverable is the network, and the remedy is to wait.

`SyncJournal::account_summary` reduces one record's standing across every peer to the single worst
answer, via `AccountSyncSummary`'s `Ord` (`Refused > Conflict > Undeliverable > Pending > Synced`) —
the one badge `tddy-accounts`' `SyncStatusSource` port reads for the Accounts screen, not a per-peer
breakdown. `None` (no badge at all) means nothing has been attempted for that record yet — no peer
configured, or none admitted — which the screen does not conflate with `Synced`.

## Wiring: where the pieces are assembled

This crate defines the ports and the engine; nothing in it constructs a real one. `tddy-daemon`'s
`credential_sync` module (`packages/tddy-daemon/src/credential_sync.rs`) is where a real
`SyncEngine` is built — the persisted transport key, an `IdentityVerifier` bridging `#keyring` 1/9's
async `KeyDirectory` into this crate's synchronous port, a `SigningPeerTransport` that signs this
daemon's own advertisement before it reaches `LiveKitPeerTransport`, and the peer-join watch that
calls `publish`. It is constructed whenever a common room exists at all, regardless of whether
`keyring.group_secret` is set — an unconfigured secret still produces a real, journaling engine that
refuses every peer, rather than no engine.

## Known limitations

Stated rather than fixed, each with its own `docs/dev/todo/` entry:

- **Exactly one signed-in subject, or sync sits out the attempt.** The wire format names no subject —
  a `(provider, account)` record is addressed to a *daemon*, not to a person on it — so a daemon with
  more than one open vault (`tddy_daemon_auth::SessionVaults`, several signed-in operators on one
  server) has no way to route an incoming record to the right one. `credential_sync::build`'s caller
  checks for exactly one open vault before every publish attempt and logs (not warns) when that does
  not hold; sync resumes once it does again.
  (`docs/dev/todo/2026-10-05-keyring-sync-single-subject-only.md`)
- **A received tombstone cannot yet be retained with its original version and `deleted_at`.**
  `SessionVault::remove` only ever mints its own tombstone from its own clock, so last-writer-wins —
  already decided correctly by `SyncEngine::receive` — would be undone by whichever daemon's
  tombstone has the newer *local* clock reading, not the one the two peers actually agreed on. Closing
  this needs a retain-with-given-version operation on `SessionVault`.
- **Nothing registers the receiving-side RPC handler yet.** `LiveKitPeerTransport::send` reaches a
  peer that has not registered `CredentialSync/Send` and gets back "unknown method", journaled
  `Undeliverable`. Wiring that handler surfaces the tombstone gap above immediately, so the two are
  meant to close together.
- **Sync fires on join, not yet on a vault change.** `credential_sync::run_publish_watch` triggers
  `SyncEngine::publish` when the common-room roster shows a genuine new arrival (polled, but acting
  only on an arrival — never on a bare tick, so this remains "on join" rather than a periodic sweep).
  No hook exists yet for "a credential was added, edited or removed while already synced"; that
  needs `tddy-credentials`' `SessionVault` to notify something, which was not added without stopping
  to ask first. Until it exists, an edit to an already-synced account propagates at the next peer
  join, not immediately.
- **The Accounts screen shows an aggregate, not a per-peer breakdown.** A deliberate scope decision:
  `AccountSyncSummary` is the worst status across every peer, not a list of every peer and its own
  status. `SyncJournal::for_record` already has the per-peer detail if a later feature wants to
  surface it.
