# Changeset: Per-daemon signing identity for session tokens

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Stack**: `#keyring` 1/9 — the root node · branch `feature/keyring/signing-key` · base `master`

## Affected Packages

- **tddy-github**: **no package documentation exists today** — the package has only
  [`docs/code-issues/`](../../../packages/tddy-github/docs/code-issues/). This node creates
  `packages/tddy-github/docs/session-token.md` describing the `v2` format, because four nodes of
  this stack rewrite the crate and there is nothing to amend
  - `src/session_token.rs` — `v1` → `v2`, Ed25519 in place of HMAC-SHA256, a key id in the payload
  - `src/auth_service.rs` — the cross-daemon verification test the format change invalidates
- **tddy-daemon-auth**: [README.md](../../../packages/tddy-daemon-auth/README.md) — owns the key material and the key-directory port
  - `src/auth.rs` — keypair load/generate, the `0600` write, `build_auth_entries` off `livekit`
  - `src/local_token.rs` — same signer, unchanged behaviour
- **tddy-daemon-livekit**: [README.md](../../../packages/tddy-daemon-livekit/README.md) — implements the port
  - publishes this daemon's public key to the common room and resolves peers' keys from it
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs` — wiring the keypair and the port into the service graph
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md)
  - `src/config.rs` — `livekit` stops being a prerequisite for authentication
- **tddy-coder**: [README.md](../../../packages/tddy-coder/README.md)
  - `src/run.rs` — `build_auth_service_entry`, the CLI's own signer construction

## Related Feature Documentation

- [PRD — Per-daemon signing identity for session tokens](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-signing-key.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [The identity boundary and the LiveKit service](../../ft/daemon/auth-livekit-services.md)
- [LiveKit peer discovery](../../ft/daemon/livekit-peer-discovery.md)
- [Tddy Desktop](../../ft/desktop/tddy-desktop-tauri.md)

## Summary

The daemon stops signing session tokens with `config.livekit.api_secret` and signs them with an
**Ed25519 keypair it generates for itself** at first boot. Tokens move from `v1` (HMAC over a secret
every daemon shares) to **`v2`** (a signature plus a key id naming the signer), and a daemon
verifying a token it did not mint resolves that key id through a **key-directory port the auth crate
owns and `tddy-daemon-livekit` implements**.

Breaking, with no fallback: `v1` tokens are rejected outright and every signed-in client re-logs in
once.

## Background

`packages/tddy-daemon-auth/src/auth.rs:68` is the whole of the coupling this node removes:

```rust
// The one secret every daemon in a deployment shares (it also signs LiveKit room JWTs).
let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
let signer = signing_secret.as_deref().map(|s| SessionTokenSigner::new(s.as_bytes()));
```

A LiveKit credential therefore gates *all* authentication — including the settings service an
operator would use to repair the configuration — the blast radius of that credential is far wider
than a room JWT needs, and there is no per-daemon identity for `#keyring` 6/9 to authenticate peers
against. The PRD's § *Background* carries the measured detail; this changeset carries the delta.

## Responsibility

**This node owns the daemon's cryptographic identity and the session-token format built on it.**

Concretely, and exclusively:

- the Ed25519 keypair: generation on first boot, load on every boot, the `0600` at-rest write, and
  the public half's published shape (SPKI DER + fingerprint);
- the `v2` session-token format — the signed region, the key id, sign and verify;
- the **key-directory port**: the trait in `tddy-daemon-auth` by which auth asks "what public key
  does daemon `kid` publish?", and its one implementation in `tddy-daemon-livekit`;
- the removal of `livekit.api_secret` from every authentication path.

## Boundaries

**Owned surface — no other node implements or changes these:**

| Symbol | Crate | Note |
|---|---|---|
| `SessionTokenSigner` / `SessionTokenVerifier` | `tddy-github` | re-signatured for Ed25519 + key id |
| the `v2` token encoding and `verify` entry point | `tddy-github` | `session_token.rs` |
| `DaemonSigningKey` (generate / load / public half) | `tddy-daemon-auth` | new |
| `KeyDirectory` (the port) | `tddy-daemon-auth` | new — **auth owns it, see below** |
| `LiveKitKeyDirectory` (the adapter) | `tddy-daemon-livekit` | new |

**Explicitly not this node's, though adjacent:**

- The GitHub OAuth flow, the device flow, `users:` mapping, the `github:` block requirement — all
  `#keyring` 2/9. A daemon still needs a `github:` block after this node.
- `GitHubTokenStore` / `FileGitHubTokenStore` and `github-tokens.json` — `#keyring` 3/9 deletes them.
- Peer *eligibility* — which peers may receive vault state — is `#keyring` 6/9. This node establishes
  only *who a daemon is*, not what that entitles it to.
- Splitting `auth.rs` (1,200 lines) or `config.rs` (1,447 production lines). Two independent records
  say the split belongs on a follow-up branch; see `## Prerequisites`.

**The boundary this node is wired against, and must not relax:**
`packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs` walks the manifest closure and
fails if `tddy-daemon-auth` lands on `tddy-daemon-livekit`'s dependency path. That is why key
distribution is a **port**, not a call: auth declares the trait, LiveKit implements it, exactly as
`SessionTokenMinter` already does. The test must pass **unchanged**.

> **The cheaper route is available and is rejected.** `tddy-daemon-auth` already declares
> `livekit = "0.7"` and `tddy-livekit`, and `oauth_loopback_tunnel.rs:11,21` already drives a `Room`
> and an `RpcClient` from inside the auth crate — so auth *could* read and publish keys against the
> room directly and the boundary test would stay green, because it constrains the opposite
> direction. The room is one transport, and a desktop or single-daemon deployment has none. A port
> keeps "where do I get a peer's public key" answerable without a room, which is the coupling this
> node exists to remove; the shortcut re-creates it somewhere new.

**A second placement constraint, from this node's own code-issue analysis.** The keypair and the
port land in `tddy-daemon-auth` and **must not** land in `tddy-daemon-kernel`: `peer_forwarding.rs`
is the kernel's sole LiveKit-SDK consumer and forces `livekit = "0.7"` onto all 14 of its
dependents, six of which use none of it, and the SDK cannot enter `tddy-tools`'
`--no-default-features` in-jail build. See
[`heavy-dependency-livekit-peer-forwarding`](../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md).

## Dependencies

**Parents**: none inside `#keyring`. This is the stack's root node, but its PR opens against
[#492](https://github.com/uppin/tddy-coder/pull/492) `feature/carve/git-plumbing` — a node of the
separate `#carve` stack — not against `master`. The whole `#keyring` line therefore lands after
`#carve` 6/10 does. That base was taken deliberately: `#carve` moves the `tddy-github` and
`tddy-core` code this stack edits, and taking the move now, while every `#keyring` node is still
docs-only, is the cheapest this rebase will ever be. It also satisfies `#keyring` 9/9's block on
#492 from the base rather than as a separate wait.

**Direct dependents**:

| Node | What it takes from here |
|---|---|
| `#keyring` 2/9 `desktop-login` | a daemon that authenticates with no `livekit:` block — barrier 2 of the three the desktop install hits |
| `#keyring` 3/9 `store` | a session-derived key the vault can be gated on, and a token format that names its signer |
| `#keyring` 6/9 `sync` | the per-daemon identity peers are authenticated *as* |

**New external dependency**: `ed25519-dalek` — **approved by the developer 2026-09-19** under
CLAUDE.md § ASK. `zeroize` for the secret key is recommended and optional. Nothing else is added;
`#keyring` 6/9's wrap-to-recipient reuses the workspace's existing pinned `rsa`.

**Nothing in this node's own work waits on an unmerged PR**, but its *base* does: the line is
`#carve` 4/10 [#498](https://github.com/uppin/tddy-coder/pull/498) → `#carve` 5/10
[#491](https://github.com/uppin/tddy-coder/pull/491) → `#carve` 6/10
[#492](https://github.com/uppin/tddy-coder/pull/492) → this node. All three must merge before any
`#keyring` node can.

## Draft PR contract

What this PR publishes in its **second commit** (wave 2), before any implementation:

**Surface**

- `tddy-github` — `SessionTokenSigner::new(&DaemonSigningKey)`, `SessionTokenVerifier` taking a key
  resolver, and the `v2` payload struct carrying `kid`. Signatures only; bodies `todo!()`.
- `tddy-daemon-auth` — `DaemonSigningKey::load_or_generate(&Path)`, its public-half accessor, and
  the `KeyDirectory` trait (`fn public_key_for(&self, kid: &str) -> Option<VerifyingKey>` in its
  async, error-carrying form).
- `tddy-daemon-livekit` — `LiveKitKeyDirectory`, declared as implementing `KeyDirectory`.

**Failing tests**

- acceptance: a `v2` token minted by daemon A verifies on daemon B once B has seen A's published key;
- acceptance: a daemon with **no `livekit:` block** registers `auth.AuthService` and answers a
  token-gated RPC;
- unit: a key id naming an unseen daemon is rejected with no fallback;
- unit: a `v1` token is rejected;
- unit: the keypair is generated once at `0600` and **reused** on restart;
- unit: `verify_rejects_a_token_with_a_tampered_signature`, rewritten so it does not depend on
  base64url canonicality (see `## Prerequisites`).

⚠ **This PR does not merge in that state.** The contract is the first push of a PR that goes on to
implement all of it in the same PR via `/green`.

## Green wave

**Wave 1 of 5** — this node needs no predecessor's behaviour, and is the only node in its wave.
Three nodes wait directly on it and eight transitively, which is what puts it first in the line.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

## Prerequisites

Open items this node runs into. Sources: `docs/dev/todo/` (146 entries screened, 14 relevant to the
stack, 9 naming this node) and `packages/*/docs/code-issues/` (102 records read, plus 5 opened by
this node's own analysis). **Nothing blocks this node.**

### ✅ RESOLVED HERE — the tampered-signature test is flaky 1 in 64 — [`2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md`](../todo/2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md)

Not optional, because this node makes it **worse** if left alone. The helper at
`session_token.rs:232` flips the signature's final base64url character `'A' ↔ 'B'`. A 32-byte HMAC
tag encodes to 43 characters whose last carries must-be-zero bits, so the flip yields a
non-canonical token — a decode failure rather than a signature failure — about 1 run in 64. An
**Ed25519 signature is 64 bytes** → 86 characters, and its final character carries **4** must-be-zero
bits, so only `A`, `Q`, `g`, `w` are legal there and an untampered tag ends in `'A'` roughly **1 run
in 4**. Shipping `v2` untouched escalates a ~1.5% flake to ~25%.

Closed by rewriting the helper to tamper with the decoded signature bytes rather than a base64url
character — in the same commit as the `v2` format, since the test is rewritten anyway.

### ✅ RESOLVED HERE — `ensure_owner_only_dir` no longer re-tightens an existing `auth_storage` — [`2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md`](../todo/2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md)

The entry lists three options and says the cost "deserves a decision rather than a silent accept".
It was written when the directory held a plaintext token file; a **private signing key** is the new
fact that settles it.

It is **not blocking**: a directory mode governs listing and traversal, not the contents of a `0600`
file, and `write_atomic_with_mode` sets the swap file's mode *before* writing rather than copying it
from a target that may not exist — so this node's key is protected by its own mode even inside a
loose directory.

Closed by the decision the entry asked for: the daemon **warns once at startup** when `auth_storage`
is more permissive than `0700`, rather than silently re-imposing a mode over an operator's
deliberate `chmod`. Small, and inside this node's own files.

### ⚠ DURING — `tddy-github` ↔ `tddy-daemon` coupling — [`2026-07-04-tddy-github-tddy-daemon.md`](../todo/2026-07-04-tddy-github-tddy-daemon.md)

This node moves the token format across that seam again. Recorded, not fixed: the entry's fix is a
crate split, which is not this stack's work. `#keyring` 6/9 runs into the same entry.

### ⚠ DURING — `TokenGenerator::generate_for` performs no authorization — [`2026-08-13-tokengenerator-generate-for-performs-no-authorization.md`](../todo/2026-08-13-tokengenerator-generate-for-performs-no-authorization.md)

`spawner.rs` passes the raw `--livekit-api-secret` on a child's command line, where
`/proc/<pid>/cmdline` exposes it to any local process. Today that leaks the fleet's session-token
signing key; after this node it leaks a **room credential only**. A real reduction in blast radius,
and explicitly **not a fix** — the missing authorization check remains open.

### ⚠ DURING — a split agent's join token carries `can_update_own_metadata` it never uses — [`2026-08-14-a-split-agent-s-join-token-carries-can-update-own-metadata-it-never-us.md`](../todo/2026-08-14-a-split-agent-s-join-token-carries-can-update-own-metadata-it-never-us.md)

Peer key publication uses participant metadata, so this node touches the grant. Recorded, not
widened: the port publishes through the existing metadata path and adds no grant.

### ⚠ DURING — the auth and LiveKit modules are over budget and were moved unsplit — [`2026-09-10-the-auth-and-livekit-modules-are-over-budget-and-were-moved-unsplit.md`](../todo/2026-09-10-the-auth-and-livekit-modules-are-over-budget-and-were-moved-unsplit.md)

`auth.rs` (1,200 lines) is the file this node edits most. The entry itself recommends the split
happen "on a follow-up branch after the stack lands, not inside it", and this changeset takes that
recommendation: **no restructure in `#keyring`**. Removing the secret-gated branches shrinks
`build_auth_entries` as a side effect, which is the only size change this node makes.

### ⚠ DURING — models/agents open items at wrap — [`2026-08-16-models-agents-open-items-at-wrap.md`](../todo/2026-08-16-models-agents-open-items-at-wrap.md) · PR-stack status polling — [`2026-07-26-pr-stack-status-polling-and-stack-hygiene.md`](../todo/2026-07-26-pr-stack-status-polling-and-stack-hygiene.md) · Tauri single-process daemon — [`2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md)

Three entries that touch this node's files without constraining it. Recorded so a reviewer sees they
were considered; none is fixed here.

### ℹ ANSWERED (in part) — session-worktree-sync gaps — [`2026-08-15-session-worktree-sync-deliberate-gaps.md`](../todo/2026-08-15-session-worktree-sync-deliberate-gaps.md) · remote-git-repo-over-LiveKit gaps — [`2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md`](../todo/2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md)

Both record that a client wanting to join `session-{id}` "has to hold `LIVEKIT_API_SECRET` and mint
for itself — which is the fleet's session-token signing key, and therefore a real widening of the
client trust surface." **This node severs exactly that equivalence**: afterwards `api_secret` is a
room credential and nothing more.

Both entries stay open. The room-ownership model they actually ask for "does not exist today" and is
"a much larger change"; only the premise quoted above is answered. `#keyring` 6/9 runs into both again.

### Code issues — 5 records, none claimed by this node

Opened 2026-09-19 by `/analyze-code-issues` on the two packages in this node's path that had never
been analyzed, against a **green baseline of 112 passed / 0 failed** from
`./test -p tddy-github -p tddy-daemon-kernel` — **scoped to those two packages**, as the
verification policy requires. Coverage was **not** collected, so every record states
`Coverage: not measured` and carries call-graph counts rather than a CRAP tier.

| Record | Verdict |
|---|---|
| [`heavy-dependency-livekit-peer-forwarding`](../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md) | ⚠ **During** — turns this node's crate placement from a preference into a constraint; see `## Boundaries` |
| [`oversized-file-config`](../../../packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md) | ⚠ During — this node edits `config.rs` and must not split it |
| [`missing-tests-real-exchange-code`](../../../packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md) | — Unrelated to this node. **Hand-off**: `#keyring` 2/9 adds the device flow to the same provider and should open the base-URL seam first, or it writes that seam twice |
| [`missing-tests-privilege-drop-resolve-pty-os-user`](../../../packages/tddy-daemon-kernel/docs/code-issues/missing-tests-privilege-drop-resolve-pty-os-user.md) | — Unrelated |
| [`misplaced-tests-privilege-drop`](../../../packages/tddy-daemon-kernel/docs/code-issues/misplaced-tests-privilege-drop.md) | — Unrelated |

Four ⚠ During records from the earlier whole-stack scan also name this node's files —
`complexity-auth-build-auth-entries` (`tddy-daemon-auth`) and
`complexity-run-build-auth-service-entry` (`tddy-coder`) most directly. Both are functions this node
already rewrites, and both shrink as the secret-gated branches go; neither needs a restructure node.

**This node claims no code-issue record.** Its wrap deletes only the two ✅ backlog entries above.

## Scope

**High-level deliverables tracking progress throughout development:**

- [x] **PRD**: [PRD-2026-09-19-keyring-signing-key.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-signing-key.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: owned surface + failing tests published (wave 2, commit 2)
- [ ] **Implementation**: `v2` format, keypair, port, adapter, rewiring across six packages
- [ ] **Backlog fix — flaky tampered-signature test**: helper rewritten, 100 consecutive runs green
- [ ] **Backlog fix — `auth_storage` posture**: startup warning when more permissive than `0700`
- [ ] **Testing**: acceptance + unit tests passing, dependency-boundary tests passing unchanged
- [ ] **Package Documentation**: READMEs and dev docs for the six packages
- [ ] **Code Quality**: `cargo clippy -p <pkg> -- -D warnings` per touched package; CI green

## Technical Changes

### State A (Current)

- `build_auth_entries` (`auth.rs:51`) reads `config.livekit.api_secret` and builds an HMAC-SHA256
  `SessionTokenSigner` from it. No `livekit:` block → `None` → every token-gated RPC refuses, and
  `auth.AuthService` is never registered at all.
- Session tokens are `v1`: a base64url payload plus a 32-byte HMAC tag. Any daemon holding the
  shared secret verifies any other daemon's token; the token names no signer.
- **Four signer construction sites**: `auth.rs` (GitHub login), `local_token.rs` (UDS-resolved OS
  user), `session_room.rs` via the `SessionTokenMinter` port, and two acceptance suites that build
  signers from a `FLEET_SECRET` constant.
- `auth_service.rs:366` pins the invariant by name:
  `a_token_minted_by_one_daemon_is_authenticated_by_another_sharing_the_secret`.
- `livekit.api_secret` does two unrelated jobs: LiveKit room JWTs **and** session-token signing.

### State B (Target)

- Each daemon holds an Ed25519 keypair in `auth_storage`, private half at `0600`, generated once and
  reused. Its public half is published as SPKI DER + fingerprint, matching the shape
  `packages/tddy-host-service/src/host_keypair.rs` already uses.
- Tokens are `v2`: the same signed region plus a **key id**, signed with the daemon's private key.
  Verification resolves the key id through the `KeyDirectory` port. An unknown key id is rejected —
  no fallback, no migration window.
- `build_auth_entries` needs no `livekit` block. A daemon with no `livekit:` at all registers the
  full service set and answers token-gated RPCs.
- `livekit.api_secret` signs LiveKit room JWTs and nothing else.

### Delta (What's Changing)

#### tddy-github
- **API**: `SessionTokenSigner` / `SessionTokenVerifier` re-signatured; `v2` payload gains `kid`.
- **Implementation**: Ed25519 replaces HMAC-SHA256 in `session_token.rs`; the tampering helper at
  `:232` stops depending on base64url canonicality.
- **Tests**: `auth_service.rs:366` is **rewritten**, not deleted — the cross-daemon property it pins
  still holds, by a different mechanism.

#### tddy-daemon-auth
- **Architecture**: owns key material and declares the `KeyDirectory` port.
- **API**: `DaemonSigningKey::load_or_generate`, public-half accessor, `KeyDirectory`.
- **Implementation**: `0600` write via `tddy_core::atomic_file::write_atomic_with_mode`;
  `build_auth_entries` off `livekit`; the `auth_storage` permissions warning.
- **Dependencies**: `ed25519-dalek` (approved); optionally `zeroize`.

#### tddy-daemon-livekit
- **Integration**: `LiveKitKeyDirectory` publishes this daemon's public key to the common room and
  resolves peers' keys from published participant metadata.
- **Dependencies**: none added — the port inverts the direction the boundary test forbids.

#### tddy-daemon
- **Implementation**: `runtime.rs` constructs the keypair and injects the adapter.

#### tddy-daemon-kernel
- **Implementation**: `config.rs` — `livekit` is no longer a prerequisite for auth. A few lines and
  their tests; **the file is not split** (see `## Prerequisites`).

#### tddy-coder
- **Implementation**: `run.rs:1131` `build_auth_service_entry` stops gating on
  `(Some(id), Some(secret))`.

## Implementation Milestones

- [ ] **M1** — `v2` format in `session_token.rs`, with the flaky helper corrected in the same commit
- [ ] **M2** — keypair generate/load, `0600` write, `auth_storage` permissions warning
- [ ] **M3** — `KeyDirectory` in `tddy-daemon-auth`; `LiveKitKeyDirectory` in `tddy-daemon-livekit`
- [ ] **M4** — rewire `build_auth_entries`, `local_token.rs`, `runtime.rs`, `run.rs`
- [ ] **M5** — migrate the two acceptance suites from `FLEET_SECRET` to per-daemon keys
- [ ] **M6** — `desktop.yaml.production` and the three docs that state the barrier, plus the two
      in-tree comments this node makes false: `packages/tddy-service/proto/auth.proto`'s
      `LiveKitTokenService` header ("The LiveKit API secret is also the HMAC key every daemon signs
      session tokens with") and `packages/tddy-github/src/token_store.rs`'s trait doc, which names
      the session token "HMAC"

## Testing Plan

### Testing Strategy

**Primary test level: acceptance (integration across daemons).** The change's whole point is a
property that only exists *between* two daemons — B verifying a token A minted, without a shared
secret. A unit test of sign-then-verify inside one process cannot fail for the reason that matters.
Unit tests cover the format's edges (unknown key id, `v1` rejection, key reuse) where a single
process is the honest scope.

### Acceptance tests

Two suites already construct signers from a `FLEET_SECRET` constant; they become the migration's
proof rather than being deleted.

- **Cross-daemon verification** — Given daemon A and daemon B with distinct keypairs sharing a
  common room, When A mints a session token and B receives it, Then B verifies it after resolving
  A's published public key. *Assertion*: the RPC succeeds, and B's key directory recorded a lookup
  for A's key id.
- **Authentication with no LiveKit block** — Given a daemon whose config has no `livekit:` key at
  all, When it starts, Then `auth.AuthService` is registered and a token-gated RPC with a valid token
  succeeds. *Assertion*: service list contains `auth.AuthService`; the RPC returns a payload, not
  `not_found`.
- **LiveKit room JWTs still work** — Given `livekit.api_secret` configured, When a room token is
  minted, Then it is accepted by LiveKit. *Assertion*: the credential's LiveKit role is untouched.

### Unit / integration tests

- An unknown key id is rejected — **no fallback to any other key**.
- A `v1` token is rejected.
- The keypair is generated once at mode `0600` and **reused** across a restart (assert the bytes are
  identical and the mode is `0600`).
- `verify_rejects_a_token_with_a_tampered_signature` — rewritten to tamper with decoded signature
  bytes; **100 consecutive runs** must pass.
- A startup warning is emitted exactly once when `auth_storage` is more permissive than `0700`.
- Both crates' `dependency_boundary_unit.rs` pass **unchanged**.

### Verification scope

`./test -p tddy-github -p tddy-daemon-auth -p tddy-daemon-livekit -p tddy-daemon -p tddy-daemon-kernel -p tddy-coder`
and `cargo clippy -p <pkg> -- -D warnings` per package. **Whole-workspace green comes from CI** via
`scripts/ci-status.sh --watch` / `--failures`, never from a local run.

## Acceptance Criteria

- [ ] A daemon with **no `livekit:` block at all** registers `auth.AuthService` and answers a
      token-gated RPC
- [ ] A daemon generates its keypair on first boot, at mode `0600`, and **reuses** it on restart
- [ ] A `v2` token minted by daemon A verifies on daemon B after B has seen A's published public key
- [ ] A `v2` token whose key id names a daemon B has **not** seen is rejected — no fallback
- [ ] A `v1` token is rejected
- [ ] `livekit.api_secret` still mints a working LiveKit room JWT
- [ ] Both crates' dependency-boundary tests pass **unchanged**
- [ ] `verify_rejects_a_token_with_a_tampered_signature` passes 100 consecutive runs
- [ ] The daemon warns once at startup when `auth_storage` is more permissive than `0700`
- [ ] Access/refresh TTLs, `kind` enforcement in both directions, and logout behaviour are unchanged

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract (owned surface + failing tests) — wave 2
- [ ] M1 — `v2` token format + flaky helper
- [ ] M2 — keypair and at-rest posture
- [ ] M3 — port and adapter
- [ ] M4 — rewire the four signer construction sites
- [ ] M5 — migrate the acceptance suites
- [ ] M6 — config template and docs
- [ ] Package documentation for the six affected packages
- [ ] `/wrap-context-docs` — deletes the two ✅ RESOLVED HERE backlog entries named above
