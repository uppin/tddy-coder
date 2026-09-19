# Changeset: Screen-sharing credentials become vault records

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 7/9 · branch `feature/keyring/screen-share` · base `feature/keyring/sync` (#513)

## Affected Packages

- **tddy-screen-sharing** (no package `docs/` directory — this node creates
  `packages/tddy-screen-sharing/docs/screen-sharing-service.md`)
  - `src/screen_sharing_vault.rs` — **deleted** (394 lines)
  - `src/screen_sharing_service.rs` — reads the credential store; `require_key` and
    `ScreenSharingKeyCache` removed
- **tddy-service**: `proto/screen_sharing.proto` — `UnlockVault` and its two messages **deleted**
- **tddy-web**: [capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md)
  - the passphrase prompt **deleted**
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs:1261` — the key-cache construction and wiring removed

## Related Feature Documentation

- [PRD — Screen-sharing credentials become vault records](../../ft/screen-capture/1-WIP/PRD-2026-09-19-keyring-screen-share.md)
- [LiveKit screen capture](../../ft/screen-capture/livekit-screen-capture.md)
- [Screen-sharing sessions](../../ft/web/screen-sharing-sessions.md)

## Summary

Makes screen-sharing the **second provider** in the credential store. `ScreenSharingVault`,
`DerivedKey` and `ScreenSharingKeyCache` are deleted, and so is `UnlockVault` — the RPC that carries
a passphrase in its request.

This is the node that proves the store is generic rather than a GitHub token file with a longer name:
if a second provider needs provider-specific machinery, 3/9 built the wrong thing.

## Background

`screen_sharing_vault.rs` is 394 lines of sound primitives — ChaCha20-Poly1305, a fresh nonce per
item, a verifier ciphertext, `write_atomic_with_mode(…, 0o600)` — around a per-session
`.screen-sharing.yaml`. 3/9 took that pattern deliberately, and fixed four of its limits while
copying it. This node retires the original.

The four, and where each goes:

| Limit today | After this node |
|---|---|
| `ScreenSharingTarget` — label, host, port, protocol, username — stored **outside** the AEAD | inside the sealed record |
| Argon2 parameters **unversioned** on disk | the store's header carries KDF name, version and parameters |
| The passphrase travels in `UnlockVaultRequest` | no passphrase exists; the session opens the vault |
| `DerivedKey` cached **un-zeroized** in `HashMap<session_id, DerivedKey>` | `SessionVault` is session-scoped and zeroizes on drop |

The third is the substantive one. A second secret, typed by a person and sent over a wire, existed
only because the daemon had no other way to know the person was present. After 2/9 and 3/9 it does.

## Responsibility

**This node owns screen-sharing's credentials, and the deletion of the machinery that held them.**

- the `screen-sharing` provider's record shape, with metadata inside the AEAD;
- the deletion of `ScreenSharingVault`, `DerivedKey`, `ScreenSharingKeyCache`, `.screen-sharing.yaml`
  and `UnlockVault`;
- the scope change from per-session to per-user targets;
- the demonstration that a second provider needs no provider-specific store code.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| the `screen-sharing` provider's record shape | `tddy-screen-sharing` |
| `ListTargets` / `AddTarget` / `RemoveTarget` over the store | `tddy-screen-sharing` |
| the deletions above | `tddy-screen-sharing`, `tddy-service`, `tddy-web`, `tddy-daemon` |

**Explicitly not this node's:**

- **The store** (3/9) — this node is a *consumer* and changes `tddy-credentials` **not at all**. If
  it needed to, that would be the finding, not the fix.
- **Propagation** (6/9) — a screen-sharing record syncs because it is a record. No
  screen-sharing-specific sync code exists.
- **The Accounts screen** (4/9) — it lists the new provider because it lists providers.
- **Every streaming path**: `StartStream`, `StopStream`, the bridge identity, track naming, the host
  targets, VNC input. This node changes **where the password comes from** and nothing else.

**The line this node must not cross**: **no migration read of `.screen-sharing.yaml`.** Migrating
requires the old passphrase, which means keeping the RPC being deleted alive for one run. Targets are
re-added once. The developer's instruction on the stack is explicit — *"The change can be breaking,
don't add fallbacks."*

## Dependencies

**Parent in the line**: `#keyring` 6/9 `sync` — [#513](https://github.com/uppin/tddy-coder/pull/513).
**A line position, not a real edge**: nothing here needs propagation. The real edge is **3/9**
[#510](https://github.com/uppin/tddy-coder/pull/510) — the store, the record model and
`SessionVault`. 4/9 [#511](https://github.com/uppin/tddy-coder/pull/511) is a soft edge: the Accounts
screen lists the new provider without changes, which is worth an acceptance test but is not a
requirement for this node to function.

Stated so a reader does not infer a dependency on 6/9 that does not exist. 6/9, 7/9 and 8/9 are
wave-4 siblings and could be worked in any order.

**Dependents**: none.

**New external dependencies: none.** This node *removes* the direct use of `argon2` from
`tddy-screen-sharing`.

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `screen_sharing.proto` with `UnlockVault` and its two messages **removed** — a deletion is the
  surface here, and there is nothing to stub;
- `tddy-screen-sharing`: the service's store-backed target functions, signatures only, bodies
  `todo!()`.

**Failing tests**

- a target's password is stored as a `screen-sharing` record and starts a stream;
- target metadata is **inside** the AEAD — tampering with `host` fails the open;
- **no RPC carries a passphrase** — asserted over the generated service descriptor, so a
  reintroduction fails rather than passes quietly;
- a target added in one session is available in the next;
- starting and stopping a stream is behaviour-identical;
- a locked vault surfaces as locked, **not** as "no targets";
- a `screen-sharing` record propagates through 6/9's engine with no provider-specific code;
- `tddy-credentials` is unchanged by this node — asserted as a review criterion, not a test.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

## Green wave

**Wave 4 of 5**, with 5/9, 6/9 and 8/9.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

**Intra-wave sort**: nothing depends on this node. It sits third — after 6/9, which changes the vault
format this node writes records into, and before 8/9, which changes no shared shape at all. 5/9 leads
the wave with one transitive dependent (9/9).

## Prerequisites

### ✅ RESOLVED HERE — the four `screen_sharing_vault.rs` limits

Not a `docs/dev/todo/` entry — `tddy-screen-sharing` has **no** `docs/` directory at all, so nothing
about this file has ever been recorded. The four limits were measured during this stack's Step 2
discovery and are listed above. This node deletes the file, which closes all four.

⚠ Recorded here rather than as a code-issue record because the analysis produced them **after** the
file was already scheduled for deletion; writing four records to delete them in the same stack would
be churn. The measurement is preserved in this changeset and in the node's discovery companion, which
is where the wrap will carry it into the change history.

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

This node *removes* lines from the recorded 806 — the key-cache construction and wiring at
`runtime.rs:1261`. Recorded, not claimed: it is smaller, not fixed.

### Unanalyzed packages

`tddy-screen-sharing` and `tddy-service` have **no `docs/` directory**, so nothing in either has been
measured. **"Not measured" is not "clean"** — this node deletes one file it did analyze and claims
nothing about the rest.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-screen-share.md](../../ft/screen-capture/1-WIP/PRD-2026-09-19-keyring-screen-share.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: the proto deletion + surface + failing tests (wave 2, commit 2)
- [ ] **Records**: targets as `screen-sharing` records, metadata inside the AEAD
- [ ] **Deletions**: `screen_sharing_vault.rs`, `DerivedKey`, `ScreenSharingKeyCache`, `UnlockVault`,
      the web prompt, the `runtime.rs` wiring
- [ ] **Scope change**: per-session → per-user targets
- [ ] **Testing**: unit + acceptance, scoped
- [ ] **Package Documentation**: `packages/tddy-screen-sharing/docs/screen-sharing-service.md` (new)
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- `.screen-sharing.yaml` per session directory, unlocked by a passphrase the browser sends in
  `UnlockVaultRequest`; `DerivedKey` cached per `session_id` in a `HashMap`, never zeroized.
- Target metadata outside the AEAD; Argon2 parameters unversioned.
- Targets are invisible in the next session.

### State B (Target)

- Targets are `provider = "screen-sharing"` records in the session-gated store, metadata sealed with
  the secret, and available across a user's sessions.
- No passphrase exists anywhere in the system.
- `tddy-screen-sharing` contains no cryptography.

### Delta (What's Changing)

#### tddy-screen-sharing
- **Deleted**: `screen_sharing_vault.rs` (394 lines), `DerivedKey`, `ScreenSharingKeyCache`,
  `require_key`, the direct `argon2` dependency.
- **Implementation**: `ListTargets` / `AddTarget` / `RemoveTarget` over `SessionVault`, keeping their
  request and response shapes.
- **Not changed**: `StartStream`, `StopStream`, bridge identity, track naming, host targets.

#### tddy-service
- **API**: `UnlockVault`, `UnlockVaultRequest` and `UnlockVaultResponse` removed from
  `screen_sharing.proto`. **Breaking, and deliberately not deprecated** — a deprecated RPC carrying a
  passphrase is still an RPC carrying a passphrase.

#### tddy-web
- **UI**: the passphrase prompt is removed. Targets load with the session. A **locked** vault renders
  as locked — never as "no targets", which would be 4/9's collapsed-state mistake in a second place.

#### tddy-daemon
- **Implementation**: `runtime.rs:1261` loses the key-cache construction.

### Scope change, recorded as a decision

Targets move from **per session** to **per user**. `vault_path(session_dir)` is what makes them
per-session today, and the credential store is per user.

⚠ The alternative — keeping per-session scoping by putting the session id in the record's metadata —
was considered and rejected: it preserves today's behaviour exactly, and today's behaviour is that a
person re-enters every target for every session. Recorded so the reviewer can overrule it; reversing
costs one metadata field.

## Implementation Milestones

- [ ] **M1** — the service reads and writes records instead of `ScreenSharingVault`
- [ ] **M2** — metadata inside the record
- [ ] **M3** — delete `UnlockVault`, its messages, the web prompt and `require_key`
- [ ] **M4** — delete `screen_sharing_vault.rs`, `DerivedKey`, `ScreenSharingKeyCache`, the wiring
- [ ] **M5** — the locked-vault rendering
- [ ] **M6** — acceptance: cross-session targets, identical streaming, propagation with no
      provider-specific code
- [ ] **M7** — `packages/tddy-screen-sharing/docs/screen-sharing-service.md`

## Testing Plan

### Testing Strategy

**The load-bearing test is a negative one over the generated descriptor**: no RPC in
`screen_sharing.proto` carries a passphrase. Asserting it over the descriptor rather than by reading
the file means a reintroduction — by a merge, by a revert, by a well-meant "just for migration" —
fails a test instead of passing review.

The rest splits the usual way: record shape and AEAD coverage are unit tests over bytes; cross-session
availability and identical streaming are acceptance tests over a wired daemon.

### Unit tests

- A target round-trips as a `screen-sharing` record; the password is the record's secret.
- Tampering with `host` in the sealed record fails the open — metadata is inside the AEAD.
- A `Locked` vault yields the locked outcome, **not** an empty target list.

### Acceptance tests

- **No RPC carries a passphrase** (over the generated service descriptor).
- A target added in one session is available in the next.
- Starting and stopping a stream is behaviour-identical: bridge identity, track name, dimensions.
- A `screen-sharing` record propagates through 6/9's engine, and **no provider-specific code exists
  in the sync path** — the test that decides whether 3/9 built something generic.

### Verification scope

`./test -p tddy-screen-sharing -p tddy-service -p tddy-daemon` and scoped clippy. For `tddy-web`, the
single spec under change. LiveKit-backed tests reuse the testkit container per CLAUDE.md.
Whole-workspace green comes from CI via `scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] A target's password is a `screen-sharing` record and starts a stream
- [ ] Target metadata is inside the AEAD — tampering with `host` fails the open
- [ ] `UnlockVault` does not exist; **no RPC carries a passphrase**
- [ ] `ScreenSharingVault`, `DerivedKey`, `ScreenSharingKeyCache` and `.screen-sharing.yaml` are gone
- [ ] A target added in one session is available in the next
- [ ] Streaming is behaviour-identical
- [ ] A locked vault renders as locked, not as "no targets"
- [ ] Screen-sharing records propagate with **no provider-specific code**
- [ ] `tddy-credentials` is unchanged by this node

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] M1–M7
- [ ] `packages/tddy-screen-sharing/docs/screen-sharing-service.md`
- [ ] `/wrap-context-docs` — this node claims **no** `docs/dev/todo/` entry and **no** code-issue
      record; the four measured `screen_sharing_vault.rs` limits are carried into the change history
      by this changeset
