# PRD: Screen-sharing credentials become vault records

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Stack**: `#keyring` 7/9 · branch `feature/keyring/screen-share` · base `feature/keyring/sync` (#513)

## Affected Features

- [LiveKit screen capture](../livekit-screen-capture.md)
- [Screen-sharing sessions](../../web/screen-sharing-sessions.md)
- [Cross-daemon session authentication](../../daemon/session-auth.md)

## Summary

Makes screen-sharing the second provider in the credential store: a target's password becomes a
`provider = "screen-sharing"` record, `ScreenSharingVault`, `DerivedKey` and `ScreenSharingKeyCache`
are **deleted**, and the `UnlockVault` RPC — which carries a passphrase in its request — goes with
them.

This is the node that proves the store is generic rather than a GitHub token file with a longer name.

## Background

### What exists

`packages/tddy-screen-sharing/src/screen_sharing_vault.rs` (394 lines) is a per-session
`.screen-sharing.yaml`, unlocked with a passphrase:

- `ScreenSharingVault::{create, unlock, add_target, list_targets, remove_target, decrypt_password}`;
- `DerivedKey(pub [u8; 32])` — Argon2id over the passphrase, documented as *"held in daemon memory
  only; never written to disk"*;
- `ScreenSharingKeyCache = Arc<Mutex<HashMap<String, DerivedKey>>>` — `session_id` → key;
- `UnlockVaultRequest { session_token, session_id, passphrase }`, whose comment says *"Called by the
  browser when the user enters the passphrase. The derived key is cached in daemon memory for the
  session lifetime."*

The primitives are sound — ChaCha20-Poly1305, a fresh nonce per item, a verifier ciphertext,
`write_atomic_with_mode(…, 0o600)`. `#keyring` 3/9 took the pattern and fixed four things while
copying it.

### The four limits, and where each one goes

| Limit today | After this node |
|---|---|
| `ScreenSharingTarget` — label, host, port, protocol, username — is stored **outside** the AEAD | metadata is inside the sealed record; tampering fails the open |
| Argon2 parameters are **unversioned** on disk | the store's header carries KDF name, version and parameters |
| The passphrase travels in `UnlockVaultRequest` | **no passphrase exists**; the user's session is what opens the vault |
| `DerivedKey` is cached **un-zeroized** in a `HashMap` keyed by `session_id` | `SessionVault` is session-scoped and zeroizes on drop |

The third is the one worth stating plainly: a second secret, typed by a person and sent over a
wire, existed only because the daemon had no other way to know the person was present. After 2/9 and
3/9 it does.

## Proposed Changes

### The record

```
provider   "screen-sharing"
account    the target id
label      the target's label
secret     the target's password
metadata   host, port, protocol, username   — inside the AEAD
```

`ListTargets`, `AddTarget` and `RemoveTarget` keep their shapes and read and write the store instead
of a file. They gain nothing and lose nothing except a passphrase.

### `UnlockVault` is deleted

Not deprecated — deleted, along with its two messages. A session already proves the person is
present, and an RPC that exists only to carry a second proof is the thing being removed. The change
is breaking, which the developer authorised explicitly.

### Decision: targets become per-user, not per-session

Today the vault is `vault_path(session_dir)` — `.screen-sharing.yaml` beside the session. Targets are
therefore **per session**, and a person re-enters them for each new one.

The credential store is per user. Moving screen-sharing into it makes a target added in one session
visible in the next, which is the behaviour a person would expect from something called "saved
targets".

⚠ **The alternative was considered and rejected**: keeping per-session scoping by putting the session
id in the record's metadata. It preserves today's behaviour exactly, and it makes every target
invisible the moment the session ends — which is the current annoyance, not a feature. Recorded so
the reviewer can overrule it; the cost of reversing is one field in `metadata`.

### No migration

Existing `.screen-sharing.yaml` files are not read. Migrating would require prompting for the old
passphrase — reintroducing the RPC this node deletes, for one run. Targets are re-added once, and
the developer's instruction on the whole stack is explicit: *"The change can be breaking, don't add
fallbacks."*

### What this gets for free

A screen-sharing record is a record, so `#keyring` 6/9 propagates it to authorised peers with the
same journal and the same tombstones, and `#keyring` 4/9's screen lists it beside GitHub accounts.
Neither needs a line of screen-sharing-specific code — which is the actual test of whether the store
is generic.

## What's Staying the Same

- Every streaming path: `StartStream`, `StopStream`, the LiveKit bridge identity, track naming, the
  host-target RPCs and VNC input. This node changes **where the password comes from**, nothing else.
- `ScreenSharingTarget`'s fields as the UI sees them.
- The vault format, key derivation and sync — 3/9's and 6/9's.

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-screen-sharing` | `screen_sharing_vault.rs` **deleted**; the service reads the store |
| `tddy-service` | `UnlockVault` and its two messages **deleted** from `screen_sharing.proto` |
| `tddy-web` | The passphrase prompt **deleted**; targets appear without one |
| `tddy-daemon` | `ScreenSharingKeyCache` construction and wiring removed from `runtime.rs` |
| `tddy-credentials` | **Unchanged** — this node is a consumer, and that is the point |

## Implementation Plan

1. The service reads and writes `SessionVault` records instead of `ScreenSharingVault`.
2. Metadata moves inside the record.
3. Delete `UnlockVault`, its messages, the web prompt and `require_key`.
4. Delete `screen_sharing_vault.rs`, `DerivedKey` and `ScreenSharingKeyCache`; remove the wiring.
5. Acceptance: a target added in one session is usable in the next.
6. Acceptance: streaming is behaviour-identical.

## Acceptance Criteria

- [ ] A target's password is stored as a `screen-sharing` record and used to start a stream
- [ ] Target metadata is **inside** the AEAD — tampering with `host` fails the open
- [ ] `UnlockVault` does not exist; no RPC carries a passphrase
- [ ] `ScreenSharingVault`, `DerivedKey`, `ScreenSharingKeyCache` and `.screen-sharing.yaml` are gone
- [ ] A target added in one session is available in the next
- [ ] Starting and stopping a stream is behaviour-identical
- [ ] A locked vault surfaces as locked, **not** as "no targets"
- [ ] Screen-sharing records propagate through 6/9 with no provider-specific code
- [ ] `tddy-credentials` is unchanged by this node

## References

- [LiveKit screen capture](../livekit-screen-capture.md)
- [Screen-sharing sessions](../../web/screen-sharing-sessions.md)
- `packages/tddy-screen-sharing/src/screen_sharing_vault.rs` — what is deleted
