# Changeset: Journaled credential propagation between daemons

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 6/9 · branch `feature/keyring/sync` · base `feature/keyring/assignments` (#512)

## Affected Packages

- **tddy-credential-sync** (**new crate**): `docs/credential-sync.md` — the engine, the journal, LWW,
  tombstones, the `PeerTransport` port
- **tddy-daemon-livekit**: [livekit-service.md](../../../packages/tddy-daemon-livekit/docs/livekit-service.md)
  - `src/common_room_supervisor.rs`, `src/livekit_peer_discovery.rs` — the adapter and the join trigger
- **tddy-credentials**: record version, tombstones, journal storage
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md)
  - `src/config.rs` — `keyring.group_secret`
- **tddy-accounts**, **tddy-web**: per-account sync status on the Accounts screen
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs` — construction and the adapter wiring

## Related Feature Documentation

- [PRD — Journaled credential propagation](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-sync.md)
- [LiveKit common-room peer discovery](../../ft/daemon/livekit-peer-discovery.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [LiveKit and auth services](../../ft/daemon/auth-livekit-services.md)

## Summary

Propagates credential records to the peers authorised to hold them — wrapped per recipient, gated by
**two independent checks**, reconciled last-writer-wins with tombstones, and **journaled on both
sides** so every node knows its sync status.

Room membership is not the gate. The developer's constraint is explicit: only peers holding a shared
secret, *"in most cases … server-side daemons"*.

## Background

`docs/ft/daemon/livekit-peer-discovery.md` § *Trust model* states the current position plainly: room
membership defines the peer group, any participant that can join may receive a forwarded
`StartSession` carrying a `session_token`, and *"there is no separate cryptographic attestation that
a participant runs `tddy-daemon`"*.

That is defensible for forwarding a session start to a host an operator picked from a list. It is not
defensible for handing out a person's GitHub credential — anything holding the room's LiveKit
credentials would receive it, and `#keyring` 1/9 exists precisely because this stack stopped treating
the LiveKit API secret as an authorisation decision.

1/9 already gives each daemon an Ed25519 identity and a `KeyDirectory`. That answers *"this message
came from that daemon"*. It does not answer *"that daemon may hold my credentials"*, which is an
authorisation question with a per-deployment answer — hence a second, configured check.

## Responsibility

**This node owns which peers may hold a credential, what crosses the wire, and how both sides know
where they stand.**

- the two checks — a configured group secret and 1/9's Ed25519 signature — and the rule that both are
  required;
- the X25519 transport key, published through `KeyDirectory` and signed by the identity key;
- per-recipient wrapping, and the receive path's re-seal under the recipient's own vault key;
- record versions, tombstones and last-writer-wins reconciliation;
- the journal, and its rendering on the Accounts screen.

## Boundaries

**Owned surface:**

| Symbol | Crate |
|---|---|
| `PeerTransport` (the port) | `tddy-credential-sync` |
| `SyncEngine`, `SyncJournal`, `SyncStatus` | `tddy-credential-sync` |
| `VaultTransportKey` (X25519, signed by the identity key) | `tddy-credential-sync` |
| `LiveKitPeerTransport` (the adapter) | `tddy-daemon-livekit` |
| record version + tombstone in the vault format | `tddy-credentials` |
| `keyring.group_secret` | `tddy-daemon-kernel` |

**Explicitly not this node's:**

- **The peer-discovery trust model for everything else.** `ListEligibleDaemons` and `StartSession`
  forwarding are untouched. This node adds a gate for credentials, and deliberately does **not**
  retrofit one onto forwarding — that is a separate change with its own blast radius.
- **The vault's key derivation** (3/9) — extended with a version and tombstones, not replaced.
- **1/9's signing key and `KeyDirectory`** — used as published.
- **The assignment resolver** (5/9), **screen-sharing as a provider** (7/9), **a second GitHub
  account** (8/9), **git/github identity** (9/9).

**Three lines this node must not cross:**

1. **The vault's data key, its KEK and the login credential never leave the daemon.** A recipient
   re-seals what it receives under its own vault key. Two daemons end up sharing a *record*, never a
   *key*, so compromising one does not unwrap the other's disk.
2. **Neither check admits a peer alone.** The group secret without the signature lets anyone replay
   an advertisement; the signature without the group secret authenticates a daemon nobody authorised.
3. **No sync path re-derives authorisation from room membership.** That is the property being
   removed; a "well, it's already in our room" shortcut would restore it.

## Dependencies

**Parent in the line**: `#keyring` 5/9 `assignments` — [#512](https://github.com/uppin/tddy-coder/pull/512).
**A line position, not a real edge**: this node needs nothing 5/9 ships. Its real edges are 4/9
[#511](https://github.com/uppin/tddy-coder/pull/511) (the Accounts screen it adds status to, and
`tddy-accounts`), 3/9 [#510](https://github.com/uppin/tddy-coder/pull/510) (the records and the
format) and **1/9** [#508](https://github.com/uppin/tddy-coder/pull/508) (the Ed25519 identity and
`KeyDirectory`, without which there is no authenticated sender and nowhere to publish a transport
key).

Stated because a reader seeing 6/9 stacked on 5/9 would reasonably infer a dependency that does not
exist: the two are wave-4 siblings and the order between them is the intra-wave sort, not a need.

**Dependents**: none.

**New external dependencies**: ⚠ an X25519 implementation is needed —
`x25519-dalek`, the companion of the `ed25519-dalek` already approved for 1/9. **CLAUDE.md § ASK
applies**, and the request is made in this node's green phase rather than assumed. The local
crates.io proxy makes the fetch cheap; the cost is the review.

## Draft PR contract

Published in this PR's **second commit**:

**Surface** — signatures only, bodies `todo!()` except where noted:

```rust
pub trait PeerTransport {          // the port; LiveKit implements it
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError>;
    fn peers(&self) -> Vec<PeerAdvertisement>;
    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError>;
}

pub struct SyncEngine;             // both checks, wrapping, reconciliation
pub struct SyncJournal;            // per (peer, provider, account)
pub enum SyncStatus { Pending, Sent, Acknowledged, Refused(RefusalReason), Conflict, Undeliverable }
pub enum RefusalReason { GroupSecretMismatch, SignatureInvalid, UnknownIdentity }
```

Record `version` and the tombstone variant land **real** in `tddy-credentials` — they are format, and
a stubbed format cannot be tested.

**Failing tests**

- a peer failing the group-secret check receives nothing, and the journal records
  `Refused(GroupSecretMismatch)`;
- a peer failing the signature check receives nothing, recorded as `Refused(SignatureInvalid)`;
- **each check alone is insufficient** — two tests, one per check passing in isolation;
- the outbound payload contains no data key, no KEK and no login credential;
- a received record is re-sealed under the recipient's own key, and the sender's key does not open it;
- two concurrent edits converge to the same record on both sides, by `updated_at`;
- **a deleted record does not resurrect** from a peer that still holds it — the tombstone test;
- the journal reports, per peer and record, what was sent, received, refused or conflicted;
- sync fires on join and on change, and no periodic sweep exists;
- acceptance: `ListEligibleDaemons` and `StartSession` forwarding behave exactly as before.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

## Green wave

**Wave 4 of 5**, with 5/9, 7/9 and 8/9.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

**Intra-wave sort**: nothing depends on this node, so it does not lead the wave; it sits second by
being the most foundational of the three with no dependents — it changes the vault format, which 7/9
and 8/9 then write records into. 5/9 leads with one transitive dependent (9/9).

## Prerequisites

### ⚠ DURING — `heavy-dependency-livekit-peer-forwarding` — [`heavy-dependency-livekit-peer-forwarding`](../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md)

The measured record: one misplaced dependency, 14 dependents, 6 paying for something they never use.
This node adds a sync engine that *could* be written directly against LiveKit and would repeat it
exactly. Recorded, **not claimed** — the existing record is not fixed here — but it is the reason the
port/adapter split is a boundary rather than a preference. 1/9's `KeyDirectory` is the precedent.

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

Construction and adapter wiring add lines to the recorded 806. Not claimed, not split; 3/9, 4/9 and
this node all record it.

### Unanalyzed packages

`tddy-daemon-livekit` carries code-issue records; the two modules this node touches
(`common_room_supervisor.rs`, `livekit_peer_discovery.rs`) are not among the symbols they name.
`tddy-credential-sync` is new. **"Not measured" is not "clean"** — this node claims nothing about
what has not been analyzed.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-sync.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-sync.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: port + engine surface + format change + failing tests (wave 2, commit 2)
- [ ] **Format**: record version and tombstones in `tddy-credentials`
- [ ] **Journal**: storage, statuses, read-back
- [ ] **Authorisation**: group secret + Ed25519 signature, both required
- [ ] **Transport key**: X25519, signed by the identity key, via `KeyDirectory`
- [ ] **Wrapping**: per recipient out, re-seal in
- [ ] **Reconciliation**: LWW + tombstones
- [ ] **Adapter**: `LiveKitPeerTransport` and the join trigger
- [ ] **UI**: per-account sync status
- [ ] **Dependency approval**: `x25519-dalek` (CLAUDE.md § ASK)
- [ ] **Testing**: unit + acceptance, scoped
- [ ] **Package Documentation**: the six packages above
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- A credential exists on exactly one daemon. A desktop user's GitHub credential is unreachable from
  a server daemon, so that daemon cannot act as them at all.
- The only peer-group notion is LiveKit room membership, which the peer-discovery doc states carries
  no attestation.
- There is no record version, no tombstone and no journal.

### State B (Target)

- A record reaches the peers that pass both checks, wrapped for each, and each side can say what it
  sent, received, refused or conflicted.
- Records carry versions; deletions carry tombstones; concurrent edits converge by `updated_at`.
- A recipient's disk is sealed by its own key, so sharing a record shares no key.

### Delta (What's Changing)

#### tddy-credential-sync (new)
- **Architecture**: owns the `PeerTransport` port; depends on `tddy-credentials`, **not** on
  `tddy-daemon-livekit`.
- **Implementation**: the two checks, per-recipient wrapping, LWW, tombstones, the journal.

#### tddy-daemon-livekit
- **Integration**: `LiveKitPeerTransport` over the common room; the advertisement rides the existing
  participant-metadata path; sync fires on join and on change.
- **Not changed**: `ListEligibleDaemons`, `StartSession` forwarding, session routing, and the
  existing trust model for all of them.

#### tddy-credentials
- **Format**: a per-record `version`, and a tombstone variant. **Deletions become tombstones**;
  without that, a removed account resurrects from a peer that still holds it and the person's removal
  silently undoes itself.

#### tddy-daemon-kernel
- **Config**: `keyring.group_secret` — required for a daemon to participate in credential sync, and
  absent means the daemon syncs nothing rather than syncing with everyone.

#### tddy-accounts / tddy-web
- **UI**: per-account, per-peer sync status on the Accounts screen, including refusals.

## Implementation Milestones

- [ ] **M1** — record version and tombstones
- [ ] **M2** — the journal: storage, statuses, read-back
- [ ] **M3** — `keyring.group_secret` and the advertisement
- [ ] **M4** — both checks, with the each-alone-insufficient tests
- [ ] **M5** — X25519 transport key via `KeyDirectory` (after the dependency is approved)
- [ ] **M6** — per-recipient wrapping and the receive-side re-seal
- [ ] **M7** — LWW and tombstone reconciliation
- [ ] **M8** — `LiveKitPeerTransport` and the join trigger
- [ ] **M9** — sync status on the Accounts screen
- [ ] **M10** — documentation, including the trust-model boundary this node does and does not change

## Testing Plan

### Testing Strategy

**The port is what makes this testable at all.** Authorisation, wrapping and reconciliation are
tested against an in-memory `PeerTransport` — a fake peer that can be made to fail exactly one check.
Only the adapter and the join trigger need a LiveKit server, and those are acceptance tests using the
testkit (`LIVEKIT_TESTKIT_WS_URL`, `#[serial]` where a room is shared), which is also why the
engine does not live in `tddy-daemon-livekit`.

**Two tests carry most of the value**, and both are assertions about what does *not* happen: that
each check alone admits nobody, and that a deleted record does not come back.

### Unit tests (`tddy-credential-sync`)

- A peer failing the group secret receives nothing; journal `Refused(GroupSecretMismatch)`.
- A peer failing the signature receives nothing; journal `Refused(SignatureInvalid)`.
- **Group secret alone** admits nobody. **Signature alone** admits nobody.
- The outbound payload contains no data key, no KEK and no login credential — asserted over the
  serialised bytes.
- A received record is re-sealed under the recipient's key; the sender's key does not open it.
- Concurrent edits converge to the same record on both sides by `updated_at`.
- **A tombstone survives a sync from a peer that still holds the record.**
- The journal reports per peer and record what was sent, received, refused or conflicted.

### Acceptance tests

- Two daemons sharing a group secret converge; a third in the same room with no group secret
  receives nothing and appears in the journal as refused.
- Sync fires on join and on change; **no periodic sweep exists**.
- `ListEligibleDaemons` and `StartSession` forwarding are behaviour-identical.

### Verification scope

`./test -p tddy-credential-sync -p tddy-credentials -p tddy-daemon-livekit -p tddy-daemon-kernel -p tddy-accounts`
and scoped clippy. LiveKit-backed acceptance tests reuse a running testkit container per CLAUDE.md
(`./run-livekit-testkit-server`, `LIVEKIT_TESTKIT_WS_URL`). For `tddy-web`, the single spec under
change. Whole-workspace green comes from CI via `scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] A peer failing **either** check receives nothing, and the journal records the refusal
- [ ] Neither check alone admits a peer
- [ ] The data key, the KEK and the login credential never leave the daemon
- [ ] A recipient re-seals under its own vault key
- [ ] The journal answers, per peer and record, what was sent, received, refused or conflicted
- [ ] Concurrent edits converge deterministically by `updated_at` on both sides
- [ ] **A deleted record does not resurrect**
- [ ] Sync fires on join and on change; there is no poll
- [ ] The Accounts screen shows per-account, per-peer status
- [ ] `ListEligibleDaemons` and `StartSession` forwarding behave exactly as before

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] Ask for `x25519-dalek` (CLAUDE.md § ASK) before M5
- [ ] M1–M10
- [ ] Package documentation for the six packages
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record
