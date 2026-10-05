# Journaled credential propagation between daemons

**Status:** Current
**Product area:** Daemon

## Summary

A daemon propagates its credential records to the peers **authorised to hold them**, wrapped per
recipient so only that peer can open what it receives, with a journal on each side so every daemon
knows what it has sent, received and not yet reconciled. Propagation goes only to peers sharing a
deployment-configured secret — in most deployments, server-side daemons — never to every participant
a LiveKit room happens to admit.

Room membership is **not** the gate. [LiveKit common-room peer discovery](livekit-peer-discovery.md)
§ *Trust model* states, and continues to state, that any participant able to join the common room may
appear in `ListEligibleDaemons` and receive a forwarded `StartSession`. That model is accepted for
forwarding a session start to a host an operator chose from a list. It is not accepted for handing out
a person's GitHub credential: anything holding the room's LiveKit credentials would receive it. This
feature adds a credential-specific gate on top of, not instead of, the existing trust model —
`ListEligibleDaemons` and `StartSession` forwarding are unchanged.

## Configuration

| YAML / setting | Role |
|---|---|
| `keyring.group_secret` | The deployment's shared answer to *"may that daemon hold my credentials?"*. **Absent means this daemon propagates credentials to nobody** — not to every daemon in the common room. The common case for leaving it unset is a desktop install with no fleet. |

## The two checks

A peer is admitted to receive credentials only once **both** hold:

| Check | Answers | Mechanism |
|---|---|---|
| Group membership | *may that daemon hold my credentials?* | an HMAC over a fresh challenge, proving the peer holds the same `keyring.group_secret` — the secret itself never crosses the wire |
| Identity | *did that daemon send this?* | [`#keyring` 1/9](../../../packages/tddy-daemon-auth/docs/auth-service.md)'s Ed25519 signature over the peer's advertisement |

**Neither check admits a peer alone.** The group secret without the signature lets anyone who
captured one advertisement replay it; the signature without the group secret authenticates a daemon
nobody in this deployment authorised. A peer failing either check receives nothing and is not synced
from, recorded in the journal as refused with the specific reason — a group-secret mismatch, an
invalid signature, or an identity the fleet has not published a key for yet.

## What travels, and what never does

Each daemon publishes an X25519 transport key, signed by its Ed25519 identity, separate from the key
that signs session tokens — rotating one does not invalidate the other. A record is sealed for one
recipient under a secret the two daemons agree on through that key.

**Travels:** a record's provider, account, label, metadata, secret and version — sealed so only the
intended recipient can open it.

**Never travels:** the vault's data key, its key-encryption key, or the login credential that seeded
it. A recipient re-seals what it receives under its **own** vault key before anything touches its
disk, so two daemons sharing a record never share a key — compromising one does not unwrap the
other's.

## Conflicts: last-writer-wins, with tombstones

`(provider, account)` is the key a record propagates under; the later edit, by its own clock, wins.
**A deletion is a tombstone, not an absence.** Without one, a removed account resurrects on the next
sync from a peer that still holds the old record, and the person's removal silently undoes itself. A
tombstone competes on the same clock a record does, so a genuinely later re-link still wins back.

## Journal and sync status

Each daemon keeps one line per peer and per `(provider, account)`: pending, sent, acknowledged,
refused (with the reason), in conflict, or undeliverable — the last two kept distinct from a refusal,
since the remedies differ (a refusal is configuration; undeliverable is the network, and often clears
on its own). The **Accounts screen** shows this as a badge per account — the single worst standing
across every peer it has been offered to, not a line per peer. No badge at all means nothing has
synced the account yet, distinct from every peer having acknowledged it.

## Trigger

Sync fires when a peer joins the common room, publishing this daemon's current vault to every peer
it can newly see. There is no periodic sweep: re-sending unchanged state on a timer would make a
peer that refused for a configuration reason look like a flapping one, and would turn one
misconfiguration into steady traffic.

## What stays the same

- The peer-discovery trust model for everything else — `ListEligibleDaemons`, `StartSession`
  forwarding and session routing are untouched. This feature adds a gate for credentials only.
- The vault's own format and key derivation (passphrase, Argon2id) — extended with a version and
  tombstones, not replaced. See [credential store](../../../packages/tddy-credentials/docs/credential-store.md).
- `AccountsService`'s existing RPCs and the project-account assignment resolver.
- `#keyring` 1/9's signing key and `KeyDirectory` — used as published, not changed.

## Known limitations

- **A daemon with more than one signed-in operator does not sync while that is true.** The wire
  format addresses a record to a daemon, not to a person on it, so a server hosting several signed-in
  operators at once has no way to route an incoming record to the right one's vault. Sync resumes
  once only one is signed in again.
- **Sync fires on a peer joining, not yet on an edit to an already-synced account.** An edit made
  while a peer relationship is already established propagates at that peer's next join, not
  immediately.
- A received deletion is reconciled correctly in memory but is not yet retained with the exact
  version and time the sending peer recorded it with — closing this needs a small extension to how
  the vault retains a tombstone.

## References

- [LiveKit common-room peer discovery](livekit-peer-discovery.md) § *Trust model* — the model this
  feature deliberately leaves unchanged for everything but credentials
- [Cross-daemon session authentication](session-auth.md)
- [Credential sync](../../../packages/tddy-credential-sync/docs/credential-sync.md) — the engine
- [Credential store](../../../packages/tddy-credentials/docs/credential-store.md) — the format this
  feature extends
- [Accounts service](../../../packages/tddy-accounts/docs/accounts-service.md),
  [Accounts screen](../../../packages/tddy-web/docs/accounts-screen.md) — where the journal surfaces
