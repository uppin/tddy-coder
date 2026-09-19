# PRD: Journaled credential propagation between daemons

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Stack**: `#keyring` 6/9 · branch `feature/keyring/sync` · base `feature/keyring/assignments` (#512)

## Affected Features

- [LiveKit common-room peer discovery](../livekit-peer-discovery.md) — its trust model is why this node needs its own gate
- [Cross-daemon session authentication](../session-auth.md)
- [LiveKit and auth services](../auth-livekit-services.md)
- **Credential propagation** — `docs/ft/daemon/credential-sync.md`, written by this change

## Summary

Propagates a daemon's credential records to the peers that are **authorised to hold them**, wrapped
per recipient, with a **journal** on each side so every node knows what it has sent, received and
not yet reconciled.

The developer's constraint is explicit: propagation goes to *"only peers that have a shared secret
(in most cases it's going to server-side daemons)"*. Room membership is **not** that secret.

## Background

### Why room membership cannot be the gate

[livekit-peer-discovery.md](../livekit-peer-discovery.md) § *Trust model* states the current position
without euphemism:

> Membership in the configured LiveKit room (same project credentials and `common_room` name)
> defines the peer group. Any participant that can join may appear in `ListEligibleDaemons` and
> receive a forwarded `StartSession` carrying the full RPC body, including `session_token`. […]
> there is no separate cryptographic attestation that a participant runs `tddy-daemon`.

That model is defensible for forwarding a session start to a host an operator chose. It is **not**
defensible for handing out a person's GitHub credential: anything holding the room's LiveKit
credentials would receive it, and `#keyring` 1/9 exists precisely because this stack stopped treating
the LiveKit API secret as an authorisation decision.

So this node introduces a gate of its own, and states plainly that the existing trust model is
unchanged for everything else.

### What 1/9 already provides

Each daemon has an **Ed25519 identity** and a `KeyDirectory` through which peers learn each other's
public halves. That gives "this message came from that daemon" — an authenticated sender. It does not
give "that daemon is allowed to hold my credentials", which is an authorisation question with a
different answer per deployment.

## Proposed Changes

### Two independent checks, both required

| Check | What it proves | Mechanism |
|---|---|---|
| **Group membership** | this peer is in the credential-sharing group | a configured group secret, proven by an HMAC over the advertisement — never sent |
| **Identity** | the message really came from that daemon | 1/9's Ed25519 signature over the advertisement |

Neither alone is enough. The group secret without the signature lets anyone replay an advertisement;
the signature without the group secret authenticates a daemon nobody authorised. A peer failing
either is **not offered records and not accepted from** — silently absent from the sync set, and
recorded in the journal as such.

### A transport key, separate from the signing key

Each daemon publishes an **X25519 transport public key**, signed by its Ed25519 identity and
distributed through 1/9's `KeyDirectory`. Records are wrapped for a recipient under a key agreed with
it.

The signing key is not reused for encryption. Mixing key uses is the kind of shortcut that is
invisible until it is not, and a separate transport key also means a transport key can be rotated
without invalidating every session token a daemon has signed.

### What travels, and what never does

**Travels**: a record's ciphertext, re-wrapped for the recipient — provider, account, label,
metadata, secret and version, sealed for that peer alone.

**Never travels**: the vault's own data key, the KEK, the login credential that derives it, or the
vault file itself. A recipient re-seals what it receives under **its own** vault key, so two daemons
sharing a record do not share a key, and compromising one does not unwrap the other's disk.

### The journal

Per peer and per `(provider, account)`, each side records: the version it last sent, the version it
last received, the outcome, and when. Statuses: `pending`, `sent`, `acknowledged`, `refused`
(authorisation), `conflict`, `undeliverable`.

The journal is what makes the answer to *"does that host have my credential yet?"* observable instead
of inferred — and it is where a refused peer is visible rather than silently missing.

### Conflicts: last-writer-wins, with tombstones

`(provider, account)` is the key; the record's `updated_at` decides. LWW is chosen because the
alternative — prompting a person to merge two copies of a secret — is worse than losing the older of
two edits to the same credential, which in practice is a re-link.

**Deletions are tombstones**, not absences. Without one, a removed account resurrects on the next
sync from a peer that still has it, and the person's removal silently undoes itself. A tombstone
carries the same `(provider, account, updated_at)` shape and wins or loses by the same rule.

### Trigger

On joining the common room, and on a record changing while joined. Not a poll: the peer set is
already event-driven, and a periodic sweep would make a refused peer look like a flapping one.

### Surfacing it

The Accounts screen (4/9) gains per-account sync status: which peers hold this record, which are
pending, and which refused it. That is what makes this node a change a person can see rather than a
background behaviour they must trust.

## What's Staying the Same

- **The peer-discovery trust model for everything else.** `ListEligibleDaemons`, `StartSession`
  forwarding and session routing are untouched. This node adds a gate for credentials only, and does
  not retrofit one onto forwarding.
- The vault format and key derivation (3/9) — extended with a version and tombstones, not replaced.
- `AccountsService`'s existing RPCs (4/9) and the assignment resolver (5/9).
- 1/9's signing key and its `KeyDirectory` — used, not changed.

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-credential-sync` (**new**) | The sync engine, the journal, LWW, tombstones, the `PeerTransport` port |
| `tddy-daemon-livekit` | The adapter implementing `PeerTransport` over the common room |
| `tddy-credentials` | Record version + tombstones; the journal's storage |
| `tddy-daemon-kernel` | `keyring.group_secret` config |
| `tddy-accounts` / `tddy-web` | Per-account sync status on the Accounts screen |

**Port here, adapter in LiveKit** — the same split 1/9 used for `KeyDirectory`, and for the same
reason: `dependency_boundary_unit.rs` forbids the edge, and a sync engine that could not be tested
without a LiveKit server would not be tested.

## Implementation Plan

1. Record version and tombstones in `tddy-credentials`.
2. The journal: storage, statuses, and reading it back.
3. `tddy-credential-sync`: the `PeerTransport` port, the advertisement, and both checks.
4. The X25519 transport key, signed by the Ed25519 identity, published through `KeyDirectory`.
5. Per-recipient wrapping and the receive path's re-seal.
6. LWW and tombstone reconciliation.
7. The LiveKit adapter and the join trigger.
8. Sync status on the Accounts screen.

## Acceptance Criteria

- [ ] A peer failing **either** check receives nothing, and the journal records the refusal
- [ ] Neither check alone admits a peer
- [ ] The vault's data key, its KEK and the login credential **never** leave the daemon
- [ ] A recipient re-seals under its own vault key — two daemons share a record, not a key
- [ ] The journal answers, per peer and record, what was sent, received, refused or conflicted
- [ ] Concurrent edits converge by `updated_at`, deterministically on both sides
- [ ] **A deleted record does not resurrect** from a peer that still holds it
- [ ] Sync happens on join and on change, not on a poll
- [ ] The Accounts screen shows per-account, per-peer status
- [ ] `ListEligibleDaemons` and `StartSession` forwarding behave exactly as before

## References

- [LiveKit common-room peer discovery](../livekit-peer-discovery.md) § *Trust model*
- [Cross-daemon session authentication](../session-auth.md)
- [LiveKit service](../../../../packages/tddy-daemon-livekit/docs/livekit-service.md)
