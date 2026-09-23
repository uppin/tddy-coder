# Changeset: Per-daemon signing identity for session tokens

**Date**: 2026-09-19
**Status**: 🚧 In Progress — implemented; awaiting CI and `/wrap-context-docs`
**Type**: Architecture Change
**Stack**: `#keyring` 1/9 — the root node · branch `feature/keyring/signing-key` · base `master`
(PR #508). It was planned on `feature/carve/git-plumbing` (`#carve` 6/10, PR #492); #492 merged on
2026-09-22 and the PR now targets `master`.

## Affected Packages

- **tddy-github**: **no package documentation existed** — the package had only
  [`docs/code-issues/`](../../../packages/tddy-github/docs/code-issues/). This node creates
  `packages/tddy-github/docs/session-token.md` describing the `v2` format (written directly — the
  developer confirmed the exception, since four nodes of this stack rewrite the crate and there was
  nothing to amend)
  - `src/session_token_v2.rs` — `v2`, Ed25519 in place of HMAC-SHA256, a key id in the payload;
    `src/session_token.rs` (`v1`) deleted
  - `src/auth_service.rs` — `new_signed` takes a `SessionTokenAuthority`; the cross-daemon test
- **tddy-daemon-auth**: [README.md](../../../packages/tddy-daemon-auth/README.md) — owns the key
  material and the key-directory port
  - `src/signing_key.rs` (new) — `DaemonSigningKey`, `KeyDirectory`, `DirectorySessionTokenVerifier`,
    `SessionTokens`
  - `src/auth.rs` — `build_auth_entries_with`, off `livekit`; the remote-git mint's identity prefix
- **tddy-daemon-livekit**: [README.md](../../../packages/tddy-daemon-livekit/README.md) — publishes
  this daemon's public key in its common-room advertisement and hands peers' back undecoded
  - `src/livekit_peer_discovery.rs` — `AdvertisedSigningKey`, `peer_signing_public_keys`, the shared
    daemon-identity rule
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/common_room_key_directory.rs` (new) — the port's adapter; `src/runtime.rs` — the wiring
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md)
  - `src/config.rs` — two doc comments; `src/daemon_identity.rs` — re-exports the split-agent prefix
- **tddy-service**: `src/participant_identity.rs` (new) — `may_be_daemon_discovery_identity`, the one
  rule peer discovery and `token.TokenService` both read; `src/token_service.rs` refuses it;
  `proto/auth.proto` header comment
- **tddy-session-lifecycle**: `split_session.rs` and the connection service verify callers and mint
  agents' credentials through `SessionTokens`; test suites migrated off the shared secret
- **tddy-worktree-service**: two doc comments; one suite migrated
- **tddy-web**: regenerated `auth_pb.ts`; the standalone connect screen's identity placeholder and
  its e2e use a `web-` identity; `cypress.config.ts` comment
- **tddy-rust-typescript-tests**: regenerated `gen/auth_pb.ts`
- **tddy-desktop**: README, `desktop.yaml.production`
- **tddy-remote-git-repo**, **tddy-vm-testkit**: now-false comments about the shared secret
- **tddy-connectrpc**, **tddy-e2e**: test fixtures ask the mint for `web-` identities

`tddy-coder` was planned here and needed no change (`run.rs` never held a session-token signer).

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
| `AdvertisedSigningKey`, `peer_signing_public_key` | `tddy-daemon-livekit` | new — the key as **opaque strings** on the advertisement |
| the `KeyDirectory` **adapter** over those strings | `tddy-daemon` | new — see below |

> **Measured correction.** The adapter cannot live in `tddy-daemon-livekit`. That crate's own
> `tests/dependency_boundary_unit.rs::does_not_reach_the_identity_boundary_it_mints_no_tokens_of_its_own`
> asserts `tddy-daemon-auth` is absent from its transitive closure — *"a room token arrives through
> the SessionTokenMinter port, so tddy-daemon-auth has no business on this path"* — and
> `KeyDirectory` is an auth-owned trait. So the livekit crate carries the key as two opaque strings
> on `DaemonAdvertisement` and offers `peer_signing_public_key`, while the crate that already
> depends on both — `tddy-daemon` — implements the port. The boundary test stays green.

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

- `tddy-github` — `SessionTokenSigner::new(SigningKey, KeyId)` and `SessionTokenVerifier::verify(
  &str, &VerifyingKey, SystemTime)`, with `key_id_of` as a separate unverified read, and the `v2`
  claims struct carrying `kid`. Signatures only; bodies `todo!()`.

  > **Measured correction.** The planned `SessionTokenSigner::new(&DaemonSigningKey)` is
  > uncompilable: `DaemonSigningKey` lives in `tddy-daemon-auth`, which *depends on* `tddy-github`.
  > The signer therefore takes raw `ed25519_dalek` types, and `DaemonSigningKey::signer()` is the
  > one-line adapter on the auth side.
- `tddy-daemon-auth` — `DaemonSigningKey::load_or_generate(&Path)`, its public-half accessor, and
  the `KeyDirectory` trait (`fn public_key_for(&self, kid: &str) -> Option<VerifyingKey>` in its
  async, error-carrying form).
- `tddy-daemon-livekit` — `AdvertisedSigningKey` plus `peer_signing_public_key(&[PeerDaemon], &str)`,
  and the two advertisement fields they read. The `KeyDirectory` *implementation* is `tddy-daemon`'s.

**Failing tests**

- acceptance: a `v2` token minted by daemon A verifies on daemon B once B has seen A's published key;
- acceptance: a daemon with **no `livekit:` block** completes a sign-in;
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
- [x] **Draft PR contract**: owned surface + failing tests published (wave 2, commit 2)
- [x] **Implementation**: `v2` format, keypair, port, adapter, rewiring — across **eight**
      packages, not six (see `## Implementation record`)
- [x] **Backlog fix — flaky tampered-signature test**: helper rewritten, 100 consecutive runs green
- [x] **Backlog fix — `auth_storage` posture**: startup warning when more permissive than `0700`
- [x] **Testing**: acceptance + unit tests passing, dependency-boundary tests passing unchanged
      (scoped local runs; see `## Implementation record` for the two environment failures)
- [ ] **Package Documentation**: READMEs and `packages/tddy-github/docs/session-token.md` done;
      the `packages/*/docs/` deltas are recorded below for `/wrap-context-docs` to apply
- [ ] **Code Quality**: `cargo clippy -p <pkg> --all-targets -- -D warnings` clean per touched
      package locally; **CI green still owed**

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
- `build_auth_entries` needs no `livekit` block. A daemon with no `livekit:` at all **signs**, so a
  sign-in completes and the token it returns resolves to the user who signed in.
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
- **Integration**: `DaemonAdvertisement` carries `signing_key_id` / `signing_public_key`, so a
  daemon's public key rides the common-room metadata it already publishes; `peer_signing_public_key`
  resolves a peer's key from the registry by the id a token names.
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

- [x] **M1** — `v2` format in `session_token_v2.rs` (the module the contract pinned), with the
      flaky helper corrected in the same commit
- [x] **M2** — keypair generate/load, `0600` write, `auth_storage` permissions warning
- [x] **M3** — `KeyDirectory` in `tddy-daemon-auth`; the advertisement fields and
      `peer_signing_public_key` in `tddy-daemon-livekit`; the adapter over them in `tddy-daemon`
- [x] **M4** — rewire `build_auth_entries`, `local_token.rs`, `runtime.rs`; `run.rs` needed no
      change (see below); `v1` deleted
- [x] **M5** — migrate the acceptance suites from `FLEET_SECRET` to per-daemon keys — twelve
      suites, not two
- [x] **M6** — `desktop.yaml.production` and the three docs that state the barrier, plus the two
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
  all, When a user signs in, Then the exchange returns a session token that resolves to that user.
  *Assertion*: `ExchangeCode` returns a token, and a token-gated RPC carrying it names the same login.

  > **Measured correction.** The plan said such a daemon "registers no session services at all".
  > It is wrong: two tests written to pin it — one asserting `auth.AuthService` is unregistered, one
  > asserting the identity function is absent — both **passed** against today's code. Registration
  > never depended on `livekit`. The real barrier is one step later, at `ExchangeCode`:
  > `FailedPrecondition: "session token signing is not configured"`, because the signing key *is*
  > `livekit.api_secret`. The two tests were removed and replaced by the two that pin that break.
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

- [x] A daemon with **no `livekit:` block at all** completes a sign-in, and the token it issues
      resolves to the user who signed in — `auth_without_livekit_acceptance.rs`
- [x] A daemon generates its keypair on first boot, at mode `0600`, and **reuses** it on restart —
      `signing_key::tests`, `auth::tests::a_token_issued_before_a_restart_still_authenticates_after_it`
- [x] A `v2` token minted by daemon A verifies on daemon B after B has seen A's published public key
      — `per_daemon_signing_identity_acceptance.rs` (B's directory is asked for exactly A's key id),
      and over a real common room in `session_room_cross_host_acceptance.rs` /
      `remote_managed_worktree_cross_host_acceptance.rs`; the key `runtime::build` advertises is the
      key it signs with — `runtime_signing_identity_acceptance.rs`
- [x] **A non-daemon participant advertising a signing key does not get its tokens accepted** — a
      signed-in web user is refused a daemon-shaped identity by `token.TokenService` and minted only
      a `web-` one, and a key advertised under that identity is never believed —
      `common_room_key_trust_acceptance.rs`, `token_service::tests`,
      `livekit_peer_discovery::tests::eligible_daemon_rejects_every_identity_a_client_facing_mint_hands_out`
- [x] A participant re-advertising a genuine key id with other bytes cannot shadow the real key, and
      a learned key keeps verifying while discovery reconnects — `common_room_key_trust_acceptance.rs`
- [x] A `v2` token whose key id names a daemon B has **not** seen is rejected — no fallback
- [x] A `v1` token is rejected — `verify_rejects_a_v1_token_as_an_unsupported_version`
- [x] `livekit.api_secret` still mints a working LiveKit room JWT — `token_service_acceptance.rs`
- [x] Both crates' dependency-boundary tests pass **unchanged**
- [x] `verify_rejects_a_token_with_a_tampered_signature` passes 100 consecutive runs
- [x] The daemon warns once at startup when `auth_storage` is more permissive than `0700` —
      `auth_storage_posture_warning_acceptance.rs` (exactly one record for `0755`, none for `0700`)
- [x] Access/refresh TTLs, `kind` enforcement in both directions, and logout behaviour are unchanged

## Implementation record

What `/green` found once the contract met the code. Each item is a place the plan above was wrong
or silent, and what was done instead.

### Measured corrections

- **The module is `session_token_v2`, and `session_token.rs` is gone.** The contract's tests import
  `tddy_github::session_token_v2::…`, so v2 kept that path and v1 was deleted outright; the crate
  root re-exports v2 under the old names (`tddy_github::SessionTokenSigner`, …). `hmac` and
  `subtle` left `tddy-github`'s manifest with it.
- **`run.rs` needed no change.** `build_auth_service_entry`'s `(Some(id), Some(secret))` gate is on
  `--github-client-id` / `--github-client-secret` — the OAuth app's credentials, which a real
  provider genuinely needs — not on LiveKit. The CLI builds an *unsigned* `AuthServiceImpl` and never
  held a session-token signer.
- **`config.rs` held no LiveKit-for-auth gate** — that lived in `auth.rs`. Its change is two doc
  comments (`auth_storage` now names the signing key; `LiveKitConfig::enabled` no longer claims
  `api_secret` signs session tokens).
- **Six signer construction sites, not four**, and **eight packages, not six.** Beyond `auth.rs`,
  `local_token.rs`, `session_room.rs`'s minter and the acceptance suites, `tddy-session-lifecycle`'s
  `split_session.rs` both verified callers' tokens and minted agents' own with `livekit.api_secret`,
  and `runtime.rs` built two more signers from it (the local socket and `local_token.LocalTokenService`).
  `tddy-session-lifecycle` and `tddy-worktree-service` are therefore touched too.
- **Twelve suites carried a shared secret, not two.** `FLEET_SECRET` / `LK_API_SECRET` session
  tokens were minted in `tddy-daemon-auth` (3 suites + `auth.rs`/`lib.rs` units), `tddy-github`
  (`auth_service.rs` units, `github_token_retention_acceptance.rs`), `tddy-session-lifecycle`
  (5 suites + `split_session.rs` and the cross-daemon units), `tddy-daemon` (4 suites) and
  `tddy-worktree-service` (1). Every one now signs with a daemon key of its own; the cross-host
  suites advertise each daemon's key on the real common room and verify through the real adapter.
- **Two contract defects, fixed in production code.** `DaemonAdvertisementWire::signing_key_id`
  lacked `#[serde(default)]`, so every advertisement without a key — any daemon not yet advertising
  one — failed to parse (8 `tddy-daemon-livekit` tests). And the `session_token_v2` tests verified at
  a fixed instant (Unix 1,800,000,000, 2027-01-15) tokens that `mint_access` stamps on the real clock,
  which is past their five-minute expiry; the fixture's `now()` returns the real clock.

### Decisions the plan left open

- **Where the key lives without `auth_storage`.** `auth_storage` when configured; otherwise
  `<tddy_data_dir>/auth/signing_key.pem` — the layout `./install` gives `auth_storage` anyway
  (`signing_key_path`). The runtime passes its own resolved data dir (`TDDY_DATA_DIR` included) so
  there is one rule, not two. Moving `auth_storage` moves the key: a daemon that finds none generates
  a new identity, and every session it issued ends.
- **First boot is race-safe.** The key is written through `write_atomic_with_mode` to a private
  staging name and **hard-linked** into place, which, unlike a rename, refuses to replace an existing
  file; the loser loads the winner's key. Two daemons (or parallel tests) booting on one data dir
  otherwise each keep a different key and one of them silently signs with a key no restart finds.
- **A key file other accounts can reach is refused, not repaired** — the contract's
  `refuses_a_key_file_other_accounts_can_read`. The `auth_storage` *directory* is only warned about,
  once, in `build_auth_entries_with` (`auth_storage_looser_than_owner_only`), per the decision in
  `## Prerequisites`.
- **The synchronous RPC gate and the asynchronous port.** `SessionUserResolver` is a sync closure
  (62 call sites), `KeyDirectory` is async. `DirectorySessionTokenVerifier::verify_now` polls the
  verification **once**; a lookup still pending is refused as `UnknownKeyId` rather than blocking an
  RPC worker. The trait's doc states the resulting contract: a directory answers from what it holds.
  The one production directory reads the registry's in-memory snapshot, so it always answers on the
  first poll. **Open question for review**: whether `KeyDirectory` should become synchronous instead.
- **`KeyDirectory::publish` on the common-room adapter** checks rather than sends. The room learns a
  daemon's key from the advertisement the discovery loop publishes on every connection, and that
  advertisement is fixed when the loop starts (`AdvertisedSigningKey` by value, per the contract), so
  `publish` refuses a key other than the advertised one. An advertised key that does not hash to the
  id it is advertised under is refused by the adapter, not handed to the verifier.
- **`AuthServiceImpl::new_signed(provider, signer, authority)`** takes a `SessionTokenAuthority` —
  a new one-method trait in `tddy-github` — so status checks and refreshes accept exactly the peers
  the RPC gate accepts. `DirectorySessionTokenVerifier` implements it.
- **One `SessionTokens` per daemon** (signer + verifier), built in `runtime.rs` and handed to
  `build_auth_entries_with`, the local socket, `local_token.LocalTokenService` and
  `DaemonSessionHost::with_session_tokens` (split and jailed-codebase agent credentials).
  `build_auth_entries(config, …)` remains for a daemon with no fleet (standalone directory).
- **`tddy_github::session_token_v2` re-exports the Ed25519 key types** (`Ed25519SigningKey`,
  `Ed25519VerifyingKey`) so a dependent can name them without depending on `ed25519-dalek`.
  `ed25519-dalek` (approved) was added to `tddy-daemon`, which decodes advertised SPKI keys; the
  `pem` feature of it and `rand_core`'s `getrandom` were enabled in `tddy-daemon-auth`. No crate
  entered `Cargo.lock`.

### Verification (local, scoped — whole-workspace health is CI's)

`./test --no-fail-fast -p <pkg>` against a reused LiveKit testkit container
(`LIVEKIT_TESTKIT_WS_URL`), scoped to the packages this node touches:

| Packages | Passed | Failed | What failed, and why |
|---|---:|---:|---|
| `tddy-github`, `tddy-daemon-auth`, `tddy-daemon-livekit`, `tddy-daemon-kernel` | 409 | 2 | Two LiveKit repro suites, `signal connection timed out` — both green on rerun |
| `tddy-session-lifecycle`, `tddy-worktree-service`, `tddy-coder` | 1,342 | 39 | 16: four unmodified sandbox suites, `sandbox RPC bridge not installed` (pre-existing, same class as the master note on `sandbox_behavior_acceptance`). 1: `action_sandbox_acceptance::sandboxed_bash_pty_action_streams_output` hung 17 min in a PTY and was killed — another worktree on an unrelated branch hung identically. 18: `tddy-remote-git-repo` not built (`./test` does not build it) — 6/6 and 12/12 once built. 4: `session_room_acceptance` read other runs' participants in a fixed-name lobby — fixed below; 22/22 on rerun, two of them after one LiveKit timeout each |
| `tddy-daemon` | 177 | 3 | `unbundle_endpoint` — fixed below. Two `session_agent_remote_acceptance` LiveKit timeouts — green on rerun |

`packages/tddy-github/tests/git_plumbing_shape.rs` passes 5/5: the four failures this changeset says
the `#carve` base carries are not present on this tree. `verify_rejects_a_token_with_a_tampered_signature`:
**100/100** consecutive runs. `cargo clippy --all-targets -- -D warnings` is clean on all seven
touched crates; `cargo build -p tddy-daemon -p tddy-coder` is clean.

**Test changes beyond the migrations** — `session_room_acceptance` names its lobby afresh per
fixture, and the three `tddy-daemon` cross-host suites name their common room afresh per run
(requested by the developer): a fixed room shared a LiveKit server with every other checkout running
the same suites. `unbundle_endpoint`'s closed module list admits `common_room_key_directory.rs`, with
the reason beside it — the adapter can live in no other crate.

### `/pr-wrap` refactor pass (2026-09-23)

What the validation findings changed, and the decisions behind it.

- **Blocker closed — the mint no longer hands out an identity discovery reads a key from.** Any
  authenticated caller could get a common-room JWT from `token.TokenService` under any
  non-`daemon-*` identity, with `can_update_own_metadata`; discovery took such a participant for a
  daemon on its self-declared metadata, and `CommonRoomKeyDirectory` then trusted its advertised
  key — a signed-in web user could forge tokens for any login fleet-wide. One predicate now decides
  both sides, `tddy_service::may_be_daemon_discovery_identity` (new `participant_identity` module in
  `tddy-service`, the lowest crate both `tddy-service`'s mint and `tddy-daemon-livekit`'s discovery
  reach; `tddy-daemon-kernel` depends on `tddy-service`, so it could not live there). Discovery
  refuses every identity it rules out; the mint refuses every identity it allows, on the daemon's
  authenticated registration and the coder's open one alike. The prefix set gained `remote-git-`:
  `MintLiveKitToken` minted that identity into the common room with the same metadata grant and
  discovery read it. `SPLIT_AGENT_IDENTITY_PREFIX` moved beside the rule; the kernel re-exports it.
  Every other mint that hands tokens to non-daemon principals was swept: split agents
  (`split-agent-`), coder/session participants (`server…`, `daemon-…`) and the remote-git client are
  in the non-daemon set; the session-sync, PTY-relay, screen-capture and agent-clone mints are made
  by holders of `livekit.api_secret`, who can mint any identity regardless (recorded, not changed).
  Legitimate callers still pass — the web mints `web-`/`browser-`; the standalone connect screen's
  placeholder and its e2e now type `web-client`.
- **`KeyDirectory` stays async** (developer decision), and **loses `publish`**: it had no production
  caller, and it could never be the thing that announces, because the discovery loop that carries
  the key lives in a crate this one may not reach. `verify_now` now documents its single poll
  precisely — why it is safe with the three directories that exist, and what an I/O-backed one must
  do.
- **Key resolution**: `peer_signing_public_keys` returns every candidate and the adapter keeps the
  one whose SPKI hashes to the id; the adapter remembers learned keys across
  `CommonRoomPeerRegistry::clear` (safe: an id names one key forever).
- **Key location refuses rather than guesses**: `signing_key_path` / `load_signing_key` bail when
  neither `auth_storage` nor `tddy_data_dir` is set, instead of repeating the runtime's
  `HOME`→`/root` rule. `runtime::build` still pins its resolved data dir before asking.
- **Dead or duplicated surface removed**: `CommonRoomKeyDirectory::advertised()`; the
  `Ed25519SigningKey` re-export; `runtime.rs`'s own signing tuple (it reads
  `AuthBuildResult::session_tokens`); the second `CommonRoomTarget::from_livekit`;
  `SessionTokenSigner::new`'s caller-supplied key id (derived now); `#[serde(default)] kind` (a
  kind-less payload is `Malformed`); seven copies of the `tddy_daemon::auth` log target
  (`AUTH_LOG_TARGET`). `split_remote_tool_env` takes `&SplitSpawnTarget`.
- **Not changed, deliberately**: `AdvertisedSigningKey`'s `Default` stays (≈15 test call sites
  across the stack's suites; its doc now says it is not a production state); the PEM read is not
  `Zeroizing` (no new dependency); `tddy-daemon`'s direct `ed25519-dalek` edge and the widened
  `unbundle_endpoint` list stand; `DaemonSigningKey::key_id()` (owned) and
  `SessionTokenSigner::key_id()` (borrowed) keep their shapes, to spare eight dependents' rebases.
- **Test isolation**: `auth_without_livekit_acceptance`, `livekit_service_registration_acceptance`
  and `embedded_runtime` keep their keys in tempdirs; the six suites that kept keys in
  `CARGO_TARGET_TMPDIR` generate them per run. The two stray untracked
  `packages/{tddy-daemon,tddy-daemon-auth}/tmp/.tddy/auth/signing_key.pem` files were deleted.

Stale statements fixed directly (not `packages/*/docs/`): `tddy-remote-git-repo`'s
`credentials.rs`, `daemon_rpc.rs` and README; `tddy-vm-testkit`'s `SESSION_TOKEN_SECRET` (a
`TODO(keyring)` to drop the now-pointless `livekit:` block once a VM-backed run can confirm it);
`remote_git_livekit_acceptance.rs`, `remote_git_relay_acceptance.rs`,
`session_agent_remote_acceptance.rs`, `remote_managed_worktree_acceptance.rs`,
`cypress.config.ts`; `docs/ft/daemon/{remote-git-repo,remote-managed-worktree,session-worktree-sync,livekit-peer-discovery}.md`;
`docs/dev/todo/2026-08-03-tddy-supervisor-vm-backed-acceptance-test.md`,
`docs/dev/todo/2026-08-15-session-worktree-sync-deliberate-gaps.md`.

**Verification of the pass (local, scoped).** The shared `tddy-livekit-testkit` container on this
machine failed every WebRTC connect (`wait_pc_connection timed out`, including an unmodified repro
suite), so the LiveKit suites ran against a fresh testcontainer (`LIVEKIT_TESTKIT_WS_URL` unset).

| Scope | Passed | Failed |
|---|---:|---:|
| `tddy-service`, `tddy-daemon-kernel`, `tddy-connectrpc` — every target | 142 · 105 · 6 | 0 |
| `tddy-github`, `tddy-daemon-auth`, `tddy-daemon-livekit` — every target | 68 · 91 · 169, + 7 dependency-boundary across the last two | 0 |
| `tddy-daemon` — lib + `unbundle_endpoint`, `common_room_key_trust_acceptance`, `runtime_signing_identity_acceptance`, the three cross-host suites, `livekit_service_registration_acceptance`, `embedded_runtime` | 68 | 0 |
| `tddy-session-lifecycle` — lib + the three `sandboxed_codebase_*` suites | 233 | 0 |
| `tddy-remote-git-repo`, `tddy-vm-testkit` — lib (no unit tests; comment-only changes) | 0 | 0 |

Not run locally, left to CI: the rest of `tddy-session-lifecycle` (its sandbox suites fail here with
`sandbox RPC bridge not installed`, pre-existing) and of `tddy-daemon`; `tddy-worktree-service`
(two comment-only test edits, compiled by clippy). `cargo clippy --all-targets -- -D warnings` is
clean on every touched crate.

Oversized-file records: new `oversized-file-session-room.md` (tddy-daemon-livekit),
`oversized-file-svc-resolve-os-user.md` (tddy-session-lifecycle) and
`oversized-file-remote-git-service.md` (tddy-worktree-service); measurement rows added to the
`config.rs`, `livekit_peer_discovery.rs`, `runtime.rs` and `split_session.rs` records. None of the
six stack-shared files was split.

### Package documentation deltas — to apply at `/wrap-context-docs`

`packages/*/docs/` is not edited directly (CLAUDE.md). Each passage below is now false:

- `packages/tddy-daemon-auth/docs/auth-service.md` § *One secret signs two things* (≈ line 35) —
  replace with the rule in this crate's README § *One key signs one thing*; the neighbour link at
  ≈ line 125 ("the other half of the one shared secret") and the `token_service_acceptance.rs` row
  (≈ line 111) no longer describe a shared signer. Add `signing_key.rs` (`DaemonSigningKey`,
  `KeyDirectory`, `DirectorySessionTokenVerifier`, `SessionTokens`, `load_signing_key`) and
  `per_daemon_signing_identity_acceptance.rs` / `auth_without_livekit_acceptance.rs`.
- `packages/tddy-daemon-livekit/docs/livekit-service.md` ≈ lines 84–85 — "the same secret that signs
  session tokens"; add `AdvertisedSigningKey`, `peer_signing_public_key`,
  `CommonRoomPeerRegistry::signing_public_key_for`.
- `packages/tddy-desktop/docs/config-resolution-and-install.md` ≈ line 41 — "`livekit.api_secret` is
  the only source of the token signer": an identity is now `github:` + `users:`.
- `packages/tddy-worktree-service/docs/remote-git-service.md` ≈ line 74 — `LIVEKIT_API_SECRET` "signs
  session tokens".
- `packages/tddy-daemon/docs/daemon-endpoint.md` — add `common_room_key_directory` and the signing
  identity's place in `runtime::build`.
- `packages/tddy-daemon-livekit/docs/livekit-service.md` — peer eligibility is
  `tddy_service::may_be_daemon_discovery_identity` (browser, coder/session, split-agent and
  remote-git identities are never daemons); `peer_signing_public_keys` returns every candidate.
- `packages/tddy-daemon-auth/docs/auth-service.md` — `KeyDirectory` resolves only (no `publish`);
  `verify_now` polls once; `signing_key_path` refuses a config naming neither `auth_storage` nor
  `tddy_data_dir`; `AUTH_LOG_TARGET`.
- `packages/tddy-daemon/docs/daemon-endpoint.md` — `CommonRoomKeyDirectory` keeps the candidate that
  hashes to the id and remembers learned keys across a reconnect.
- `packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs:67` — its doc comment still says
  `config.livekit.api_secret` signs session tokens. **Developer call**: the file must stay
  unchanged in this node, so the comment is stale until someone decides to touch it.
- Feature docs the PRD replaces at wrap: `docs/ft/daemon/session-auth.md` (lines 17, 98–99),
  `docs/ft/daemon/auth-livekit-services.md` § *One secret signs two things*,
  `docs/ft/daemon/daemon-settings.md` line 59, `docs/ft/daemon/livekit-peer-discovery.md`
  § *Trust and security*.

## Validation Results

> **Addressed by the `/pr-wrap` refactor pass of 2026-09-23** — see § Implementation record →
> *`/pr-wrap` refactor pass* for what was fixed, what was deliberately left, and why. The findings
> below are kept as the validators recorded them.

### validate-changes

**Last run:** 2026-09-23 · base `origin/master` (`77187dbe`; #492 merged 2026-09-22, so the PR now
targets `master`) · range: this PR's own commits only (7 pushed, up to `abf1b749`, plus a local
unpushed `c7ecf83e` regenerating `auth_pb.ts`) · leak check ✅ · 1 deletion
(`packages/tddy-github/src/session_token.rs`, planned: "v1 deleted") ·
`git diff origin/master..HEAD -- '*/dependency_boundary_unit.rs'` empty ✅.
Scoped re-run: `tddy-github --lib session_token_v2` 13/13, `tddy-daemon-auth` lib 66/66 +
`per_daemon_signing_identity_acceptance` 3/3 + `auth_without_livekit_acceptance` 2/2.
**Risk: 1 blocker · 4 should-fix · 6 nit.**

**Blocker**

- **Any signed-in web user can forge session tokens for any login on every daemon in the fleet.**
  `CommonRoomKeyDirectory::public_key_for` (`tddy-daemon/src/common_room_key_directory.rs:76`)
  trusts any key any eligible common-room participant advertises. Eligibility
  (`livekit_peer_discovery.rs:600` `peer_daemon_from_participant_fields`) is an identity-prefix
  deny list plus self-declared metadata. `token.TokenService` (`tddy-service/src/token_service.rs:68`)
  mints a common-room JWT for **any authenticated caller** under **any identity but `daemon-*`**,
  with `can_update_own_metadata: true` (`tddy-livekit/src/token.rs:61`). So a user holding a valid
  access token asks for `room=<common_room>, identity=evil`, joins, advertises its own keypair,
  then mints `v2` tokens claiming any `login`, and every daemon accepts them and maps them to that
  user's OS account. Under `v1`, forging a token meant holding `api_secret`; web users never
  had it. This PR makes that escalation possible, and it cannot be deferred to `#keyring` 6/9
  (6/9 covers *eligibility for vault state*; this is authentication). Fix, in order of
  preference: (a) `TokenService` refuses the common-room name to non-`web-*` identities, or grants
  `can_update_own_metadata: false` there; (b) pin peer keys to a trust anchor that is not
  self-declared, e.g. TOFU per `host_id` in the durable host registry, or an operator allow-list;
  plus an acceptance test: "a non-daemon participant advertising a key does not get its tokens
  accepted".

**Should-fix**

- `livekit_peer_discovery.rs:282` `peer_signing_public_key`: first match wins. A participant that
  re-advertises a genuine kid with other bytes makes that daemon's tokens fail fleet-wide, because
  the adapter rejects the mismatch and the error surfaces as `UnknownKeyId`. That is a denial of
  service, not a bypass. Fix: return every candidate, and have the adapter keep the one whose SPKI
  hashes to the kid.
- **Availability regression.** `registry.clear()` (`livekit_peer_discovery.rs:871`) runs on every
  discovery-cycle end, so while a verifying daemon reconnects it refuses every peer's token,
  including 24 h split-agent tokens (`SPLIT_AGENT_TOKEN_TTL`). A kid is content-addressed ("an id
  names exactly one key, forever"), so the adapter can safely keep a kid→key cache that survives
  `clear()`.
- **Stale false statements not in the wrap-delta list**, so wrap will miss them.
  `tddy-remote-git-repo/src/credentials.rs:6-8`, `src/daemon_rpc.rs:8-9` and `README.md:46-48`
  all say "HMAC key every daemon signs session tokens with". `tddy-vm-testkit/src/test_host_vm.rs:36-42`
  says `SESSION_TOKEN_SECRET` is "the HMAC secret… exposed so the host can mint its own tokens",
  which is no longer possible. `tddy-worktree-service/tests/remote_git_livekit_acceptance.rs:12-15`
  (touched by this PR), `remote_git_relay_acceptance.rs:319`,
  `tddy-daemon/tests/session_agent_remote_acceptance.rs:324`,
  `tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs:732`,
  `tddy-web/cypress.config.ts:729` ("Session-token signing reads `livekit.api_secret`") and
  `tddy-daemon-livekit/tests/dependency_boundary_unit.rs:67` (developer call, since that file must
  stay unchanged) are also stale. Add to the wrap list: `docs/ft/daemon/remote-git-repo.md:218`,
  `remote-managed-worktree.md:528`, `session-worktree-sync.md:416`,
  `docs/dev/todo/2026-08-03-…:64` and `2026-08-15-session-worktree-sync…:48`.
- **Generated code.** CI "Generated code" fails on the pushed `abf1b749`. Local commit `c7ecf83e`
  regenerates both `auth_pb.ts` files, but it is **not pushed** yet.

**Nit**

- `KeyDirectory` is async while every gate is sync (`verify_now` polls once, `signing_key.rs:388`).
  Safe today, because the only production directory never awaits. But a future I/O-backed
  directory would silently refuse every peer token. `KeyDirectory::publish` has **no production
  caller**, and the adapter's `publish` only compares values. Recommend a sync
  `fn public_key_for(&self, &KeyId) -> Option<VerifyingKey>` and dropping `publish` until
  `#keyring` 6/9 needs it.
- `signing_key.rs:95` repeats the `HOME`→`"/root"` fallback from `runtime.rs:569`. It is
  unreachable from the runtime, which injects `tddy_data_dir`, but it is a second copy of the rule.
- `signing_key.rs:114`: the PEM read from disk is a plain `Vec<u8>`, not `Zeroizing`. The mode
  check runs after the read. The file owner is not checked.
- `common_room_key_directory.rs` widens `unbundle_endpoint`'s closed list claiming it "can live
  nowhere else". `tddy-session-lifecycle` already depends on both crates. `tddy-daemon` also gains
  a direct `ed25519-dalek` dependency, which a `tddy_github` SPKI-decode helper would avoid.
- Red-phase wording left after green: `auth_without_livekit_acceptance.rs:3-9` and
  `per_daemon_signing_identity_acceptance.rs:4-9` still say "Today it cannot".
- This changeset's header still says the base is `feature/carve/git-plumbing`, and it says #492
  "is red". `## Affected Packages` lists `tddy-coder`/`run.rs` and `local_token.rs` (both
  unchanged) and omits `tddy-session-lifecycle`, `tddy-worktree-service`, `tddy-desktop` and
  `tddy-service`. `packages/tddy-github/docs/session-token.md` was written directly under
  `packages/*/docs/`; confirm the CLAUDE.md exception.

**Stack boundary:** Responsibility delivered (keypair, `v2`, port + adapter, `api_secret` off
every auth path). No `todo!()` or `unimplemented!()` in the touched `src/`. No `#keyring` 2/9 or
3/9 surface crept in. Dependency-boundary tests are unchanged. Every deletion is planned. Gap vs
plan: the plan's cross-daemon acceptance assertion ("B's key directory recorded a lookup for A's
key id") is not asserted. No test covers the trust anchor, meaning who may advertise a key.

### analyze-clean-code

**Last run:** 2026-09-23 · **Score: 8/10 (B)** · scope: non-test source in `origin/master..HEAD`,
code this PR added. File-length gate not re-run (deferred files noted in the stack rule); fixes
for the six stack-shared files are in-function only.

Counts: 1 should-fix (must-refactor metric), 4 should-fix (needs attention / accuracy), 7 nits.

**Should-fix**
- `tddy-session-lifecycle/src/split_session.rs:455` `split_remote_tool_env` — 5 → **6 params**.
  Take `target: &SplitSpawnTarget<'_>` (already bundles `session_id`, `codebase_instance_id`,
  `codebase_session_id`, `session_token`) → 3 params; `prepare_split_agent_wiring:634` passes
  `target` straight through instead of destructuring. `prepare_split_agent_wiring:610` also grew
  7 → 8 (pre-existing `allow(too_many_arguments)`).
- `tddy-daemon-auth/src/signing_key.rs:86-98` `data_dir` — doc claims "the same rule the runtime
  applies", but it omits `TDDY_DATA_DIR` and duplicates `runtime.rs:565 tddy_data_dir_for`
  including the `"/root"` HOME fallback. Runtime papers over it (`runtime.rs:612`). Fix the doc to
  say it is the no-env rule, or make it `expect` `tddy_data_dir` set by the caller.
- `tddy-daemon-auth/src/signing_key.rs:174-215` `generate_into` — 42 lines (needs attention).
  Extract the `AlreadyExists` arm into `fn adopt_concurrent_key(path) -> Result<Self>`.
- `tddy-daemon-auth/src/auth.rs` `AuthBuildResult::session_tokens` — never read in production
  (only `auth.rs:647` test); `runtime.rs` carries its own `signing` tuple instead. Read
  `auth_result.session_tokens` in `runtime.rs:760` or drop the field.
- `tddy-daemon/src/runtime.rs:625-652` — `CommonRoomTarget::from_livekit` is now evaluated twice
  (`:625` and `:778`), and the `(Some, Some)` / `_ => None` match hides impossible mixes. Make
  `peer_registry` an `Option<(CommonRoomTarget, Arc<Registry>)>` and match once.

**Nits**
- `signing_key.rs:252-253, 279-280` — `0o777` / `0o077` repeated; name `PERMISSION_BITS` and
  `GROUP_OR_OTHER_BITS`.
- `"tddy_daemon::auth"` log target repeated 7× across `signing_key.rs`, `auth.rs`, `runtime.rs`
  — one `pub const AUTH_LOG_TARGET` in `tddy-daemon-auth`.
- `split_session.rs:368-385` `verified_caller` — `"cannot wire a split session: "` prefix
  repeated 6×; map the error to its tail, then `format!` once.
- `common_room_key_directory.rs:22-27` and `:63-66` — two literal `AdvertisedSigningKey`
  constructions; `publish` can compare `advertised_signing_key`-shaped output from one helper
  `fn advertise(key_id, &VerifyingKey)`. `:99/:102` computes `KeyId::of(&key)` twice — bind it.
- `livekit_peer_discovery.rs:545` `signing_public_key_for` — clones every `PeerDaemon` into a
  `Vec` to call the slice helper; search `.values()` under the read guard instead.
- `session_token_v2.rs:228` `SessionTokenSigner::new(key, key_id)` takes a derivable value only to
  `assert_eq!` it — derive `key_id` inside; `signing_key.rs:137` then stops passing it. Also
  `DaemonSigningKey::key_id()` returns an owned clone while `SessionTokenSigner::key_id()` returns
  `&KeyId` — pick one.
- Comment accuracy: `session_token_v2.rs:71-72` says base64url then "truncated to 16 bytes" —
  truncation precedes encoding; `svc_resolve_os_user.rs:126-127` leaves a reflow break mid-sentence
  ("Each is also logged here," / "because…").

Pre-existing, grown slightly, not scored against this PR: `auth.rs build_auth_entries_with`
(91 lines, +5), `runtime.rs build` (hundreds of lines, +~60 for the signing block — a same-file
private `fn session_signing(..)` would absorb it if the stack allows).

### validate-tests

**Last run:** 2026-09-23 · **Status: ⚠ 1 blocker, 6 should-fix, 6 nits** · scope: every test file
and `#[cfg(test)]` module in `origin/master..HEAD` (~65 new or rewritten test functions, plus the
fixture-level migration of 20 suites). Static review only. No tests were run.

**Confirmed sound:** the tampered-signature helper decodes, flips `signature[0] ^= 0x01` and
re-encodes (`session_token_v2.rs:658`), so it no longer depends on base64 canonicality. The v1
test asserts `UnsupportedVersion`, not a signature failure. The unknown-kid test asserts
`UnknownKeyId(stranger)` while B's directory holds other keys, so a "try every key" fallback would
give `InvalidSignature` and fail it. `auth_without_livekit` goes through the served `ExchangeCode`
and resolves the token to the same login through `build_auth_entries`' resolver. The
dependency-boundary suites are unchanged. Fresh room names per run and per fixture remove the
shared-lobby collisions. Given/When/Then is consistent throughout.

**Blocker**
- ⚠ AC "warns once at startup when `auth_storage` is more permissive than `0700`" is checked but
  **no test pins it**. `signing_key.rs:548/565` test only the pure predicate
  `auth_storage_looser_than_owner_only`. Nothing asserts that `build_auth_entries_with`
  (`auth.rs:122`) emits it, or that it is emitted exactly once. Fix: capture `log` (a test logger)
  around one `build_auth_entries_with` on a `0755` `auth_storage` and assert exactly one warning
  record naming `755`, plus zero records for `0700`. If "once per process" is the contract, also
  assert that building twice does not warn twice. Otherwise uncheck the AC.

**Should-fix**
- `tests/per_daemon_signing_identity_acceptance.rs:27` — the planned assertion "B's key directory
  recorded a lookup for A's key id" is missing. `InMemoryKeyDirectory` (`:126`) keeps no lookup
  log. Fix: add `looked_up: Mutex<Vec<KeyId>>` and push in `public_key_for` (`:140`), then assert
  `== [daemon_a.key_id()]` alongside the login. This also proves B's own-key short-circuit did not
  do the work. The `tddy-daemon` cross-host suites do not assert it either.
- `signing_key.rs:452` `generates_a_keypair_on_first_use_and_reuses_it_across_restarts` compares
  only `key_id()`. The Testing Plan asks for **identical bytes and mode `0600` on reuse**. Fix:
  read the file before and after the second `load_or_generate`, then assert
  `(bytes_after == bytes_before, mode_after) == (true, 0o600)`. The mode test at `:483` only covers
  the first write.
- `signing_key.rs:502` `refuses_a_key_file_other_accounts_can_read` asserts `is_err()` only. Any
  failure passes it, a PEM parse error included. The "no repair" its comment promises is not
  asserted. Fix: assert the error names `mode 644`, and that the file's mode is still `0o644`
  afterwards.
- `tests/auth_without_livekit_acceptance.rs:103` — the config sets neither `auth_storage` nor
  `tddy_data_dir`, so the key is generated in the **cwd-relative debug default**
  (`packages/tddy-daemon-auth/tmp/.tddy/auth/signing_key.pem`, present on disk now) and reused
  across runs. The second test passes partly on a previous run's key, and a loosened leftover
  file fails it permanently. The same new behaviour makes the unchanged `runtime::build` suites
  (`livekit_service_registration_acceptance.rs`, `embedded_runtime.rs`) leave
  `packages/tddy-daemon/tmp/.tddy/auth/signing_key.pem`. Fix: write
  `auth_storage: <tempdir>/auth` (still no `livekit:`, so the claim is intact), and give those
  runtime suites a `tddy_data_dir` tempdir.
- `CARGO_TARGET_TMPDIR` identities: `session_room_cross_host_acceptance.rs:70`,
  `remote_managed_worktree_cross_host_acceptance.rs:87`, `split_session_resume_acceptance.rs:107`,
  and `sandboxed_codebase_{lifecycle:54,placement:84,seatbelt:92}_acceptance.rs`. Keys persist in
  `target/tmp/` across runs (9 files present) and are shared by every run on that target dir.
  Persistence is only needed within one process. Fix: back `the_signing_key_of` with a
  `OnceLock<TempDir>` (or a `OnceLock<Mutex<HashMap<instance_id, DaemonSigningKey>>>`).
- AC 3 "over a real common room": the cross-host suites hand-wire `CommonRoomKeyDirectory`,
  `SessionTokens` and `advertised` exactly as `runtime.rs` does. No test drives
  `runtime::build`'s signing block, so a regression there (a `StandaloneKeyDirectory` with a
  registry, or a default `AdvertisedSigningKey` passed to discovery) would stay green. Fix: a
  `runtime::build`-level assertion that a daemon with `github:` + `livekit.common_room` advertises
  its key id, or name the gap in the AC.

**Nits**
- `signing_key.rs:523` `the_key_id_is_the_one_its_own_tokens_carry` never mints a token, although
  its When says "its signer stamps a token". Mint one and compare
  `SessionTokenVerifier::key_id_of(&token)` with `daemon.key_id()`.
- `common_room_key_directory.rs:127` asserts `is_err()`, so a base64 or SPKI failure would also
  pass. Assert that the message contains "which is not that key". Also add a
  `CommonRoomKeyDirectory::public_key_for` test through a registry snapshot, covering both a
  genuine key and a mismatched one.
- `session_token_v2.rs:532` — `assert_eq!(KeyId::of(&first), KeyId::of(&first))` is close to a
  tautology. Assert one tuple, or drop the equality half.
- Missing edges: expiry at the `now == exp` boundary; an unknown `v3.` tag giving
  `UnsupportedVersion` for a well-formed token; v1 rejection at the resolver or
  `DirectorySessionTokenVerifier` level (today it is shown only on the pure verifier); a legacy
  advertisement with no signing fields still parses (the `#[serde(default)]` defect has no
  dedicated regression test); two peers advertising one kid (`peer_signing_public_key` returns the
  first `HashMap` hit, so an impostor can nondeterministically shadow the genuine key).
- The same in-memory `KeyDirectory` fake appears three times (`per_daemon_signing_identity_acceptance.rs:126`,
  `cross_daemon_session_token_acceptance_tests.rs` `AFleet`, and `auth_service.rs` `TrustsKeys`
  as an authority). Consider one test-support fake in `tddy-daemon-auth`.
- `auth_without_livekit_acceptance.rs:26` is subsumed by `:62` (`sign_in` already `expect`s a
  successful exchange). Keep it only if the first failure message is the one you want reviewers
  to see.

### validate-prod-ready

**Last run:** 2026-09-23 · **Status: ⚠ Gaps, no blockers** · 0 blockers, 4 should-fix, 6 nits ·
scope: non-test files in `origin/master..HEAD` (~30 production files; `tests/`, `#[cfg(test)]`
modules and `cross_daemon_session_token_acceptance_tests.rs` excluded). Local gate:
`cargo clippy -p tddy-github -p tddy-daemon-auth --all-targets -- -D warnings` clean (scoped;
the other touched crates rely on the implementation record's clippy run and CI).

| Category | Count | Status |
|---|---:|---|
| Mock / fake code in production | 0 | ✅ (`StubGitHubProvider` is the pre-existing configured `github.stub` provider) |
| Dev fallbacks | 1 should-fix | ⚠ |
| TODO / FIXME / `todo!` / `unimplemented!` / `TODO(signing-key)` | 0 | ✅ |
| Unused code | 3 should-fix, 3 nits | ⚠ |
| Debug output (`println!`/`eprintln!`/`dbg!`) | 0 | ✅ |
| `.unwrap()`/`.expect()` on fallible I/O | 0 | ✅ (remaining `expect`s are infallible encodes or lock poisoning, as elsewhere) |
| Secrets logged | 0 | ✅ (only key ids and paths are logged; errors never carry key bytes) |

**v1 / shared-secret removal — clean.** `session_token.rs` is deleted, `hmac`/`subtle` are gone
from `tddy-github` (the remaining `hmac` users are `tddy-telegram`'s, unrelated), no production
path reads `livekit.api_secret` for session tokens, no verifier tries a second key, and a `v1`
token is refused as `UnsupportedVersion`. A lookup error in `DirectorySessionTokenVerifier` is
logged and refused, never retried under the local key. `load_or_generate` refuses an unwritable
path, a malformed file and a group/other-readable file rather than regenerating or repairing.

**Key location — a documented default, not a fallback.** `auth_storage`, else
`<tddy_data_dir>/auth/signing_key.pem` is chosen from configuration, never from a failure (an
unwritable `auth_storage` fails startup; it does not drop to the data dir). It is stated in
`config.rs` (`auth_storage` doc), `desktop.yaml.production`, the READMEs and the PRD, and matches
`./install`'s layout. What *is* fallback-shaped is the helper under it — see the first should-fix.

**Should-fix**
- `tddy-daemon-auth/src/signing_key.rs:89-98` `data_dir` — a second copy of
  `runtime.rs:565 tddy_data_dir_for` (without `TDDY_DATA_DIR`) ending in
  `HOME` → `"/root"`. Production never reaches it (runtime pins `auth_config.tddy_data_dir`,
  `runtime.rs:612`), so it exists only for other callers of `load_signing_key`, which would
  silently place a private key under `/root/.tddy/auth` when `HOME` is unset. Fix: make
  `load_signing_key` refuse `tddy_data_dir: None` with no `auth_storage` (the caller resolves the
  data dir), or move the one rule into `tddy-daemon-kernel` and call it from both places.
- `tddy-daemon-auth/src/signing_key.rs:313` `KeyDirectory::publish` — never called in production;
  its doc ("Called on every (re)connection") is false: the key reaches peers through
  `AdvertisedSigningKey` in the discovery loop, and `CommonRoomKeyDirectory::publish`
  (`common_room_key_directory.rs:62`) only compares. Fix: drop `publish` from the port (and the
  two impls), or call it from the discovery loop and correct the doc.
- `tddy-daemon/src/common_room_key_directory.rs:51` `CommonRoomKeyDirectory::advertised()` — no
  caller anywhere; `runtime.rs` calls `advertised_signing_key(&key)` directly. Delete it.
- `tddy-daemon-auth/src/auth.rs:41` `AuthBuildResult::session_tokens` — read only by an auth.rs
  test; `runtime.rs` carries its own `signing` tuple (same as analyze-clean-code's finding). Read
  it in `runtime.rs:759` or delete it.

**Nits**
- `tddy-github/src/session_token_v2.rs:35` — `Ed25519SigningKey` re-export has no user
  (`Ed25519VerifyingKey` is used by one test module). Drop the unused alias.
- `session_token_v2.rs:277,288` — `mint_with_issued_at` / `mint_kind_with_issued_at` are public
  test-only clock seams (carried from `v1`; production calls neither). Acceptable; consider
  `#[doc(hidden)]` or a `test-support` feature.
- `session_token_v2.rs:150` — `#[serde(default)] kind` is a `v1`-era compatibility default on a
  brand-new format; every `v2` signer writes `kind`, so a payload without it is not a real
  token. Remove the default so a kind-less payload is `Malformed`.
- `tddy-daemon-livekit/src/livekit_peer_discovery.rs:203` — `Default` on `AdvertisedSigningKey`
  ("identity not wired yet") is used only by test call sites; production always passes a real key.
  Fine as is, or build the empty value in the tests.
- `tddy-daemon/src/runtime.rs:664` — the `None` arm calls `build_auth_entries`, which only
  re-checks `github.is_none()` and returns `unauthenticated()`; `build_auth_entries` now has no
  production caller beyond this arm. Either inline, or keep as the documented standalone entry.
- `packages/tddy-github/docs/session-token.md` — new file written directly under
  `packages/*/docs/` (outside `code-issues/`). The changeset planned it, but CLAUDE.md routes
  package docs through `/wrap-context-docs`; confirm the exception with the developer or move it
  to the deltas list.

Noted for the security/test reviewers, not a readiness defect: `peer_signing_public_key` returns
the first peer advertising a kid, so a peer re-advertising a genuine kid with other bytes makes
that daemon's tokens fail to verify (a denial, never a bypass — `decode_advertised_key` rejects the
mismatch). Returning every candidate and letting the adapter keep the one that hashes to the kid
closes it.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract (owned surface + failing tests) — wave 2
- [x] M1 — `v2` token format + flaky helper
- [x] M2 — keypair and at-rest posture
- [x] M3 — port and adapter
- [x] M4 — rewire the signer construction sites (six, not four)
- [x] M5 — migrate the acceptance suites
- [x] M6 — config template and docs
- [ ] Package documentation — READMEs done; apply the `packages/*/docs/` deltas below at wrap
- [x] `/pr-wrap` refactor pass — the blocker, should-fixes and cheap nits (see Implementation record)
- [ ] CI green (`scripts/ci-status.sh --watch`) — whole-workspace health is CI's to report
- [ ] `/wrap-context-docs` — deletes the two ✅ RESOLVED HERE backlog entries named above
