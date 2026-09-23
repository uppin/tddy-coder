# Per-daemon signing identity for session tokens - PRD

**Date**: 2026-09-19
**PRD Type**: Technical Improvement
**Stack**: `#keyring` 1/9 — the root node

## Affected Features

- **Primary**: [Cross-daemon session authentication](../session-auth.md) — the token format, the
  signing key, and the mechanism by which one daemon accepts another's token all change. The
  document's central claim ("every daemon verifies both independently using the secret they all
  share") is replaced rather than amended.
- **Primary**: [The identity boundary and the LiveKit service](../auth-livekit-services.md) — its
  § *One secret signs two things* becomes § *One key signs one thing*. `livekit.api_secret` reverts
  to signing LiveKit room JWTs and nothing else.
- **Related**: [LiveKit peer discovery](../livekit-peer-discovery.md) — its § *Trust and security*
  currently concedes the room is "not a substitute for cryptographic proof that a participant runs
  authentic `tddy-daemon` software". After this change a forwarded token is verified against a
  pinned per-daemon public key, which is that proof for the token path specifically.
- **Related**: [Tddy Desktop](../../desktop/tddy-desktop-tauri.md) — a desktop install stops needing
  an invented `livekit.api_secret` before any token-gated RPC will answer. This removes one of the
  three barriers that doc records; `#keyring` 2/9 removes the other two.
- **Related**: [Daemon settings](../daemon-settings.md) — the settings service is token-gated, so it
  is among the services that become reachable on a daemon with no `livekit:` block at all.

## Summary

The daemon signs session tokens with `config.livekit.api_secret` — a secret chosen precisely
*because* every daemon in a deployment already shares it, which is what lets a token minted by one
daemon verify on another. That coupling is the thing this node removes.

Each daemon instead generates an **Ed25519 keypair** at first boot, keeps the private half in
`auth_storage` at mode `0600`, and publishes the public half. Session tokens move from `v1`
(HMAC-SHA256 over a shared secret) to **`v2`** (Ed25519 signature plus a key id naming the signer).
A daemon verifying a token it did not mint resolves the key id against the public keys its peers
published, through a port the auth crate owns.

This is a breaking change to the token format and to configuration, taken deliberately and with no
fallback path, per the repo's standing policy of migrating every consumer in the same change.

## Background

`packages/tddy-daemon-auth/src/auth.rs:68` is the whole of the coupling:

```rust
// The one secret every daemon in a deployment shares (it also signs LiveKit room JWTs).
let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
let signer = signing_secret.as_deref().map(|s| SessionTokenSigner::new(s.as_bytes()));
```

Three costs follow from it, and they are why this is the stack's root node rather than a tidy-up:

1. **A LiveKit credential gates all authentication.** No `livekit:` block means no signer, which
   means every token-gated RPC refuses — *including the settings service an operator would use to
   repair the configuration*. A fresh `./install --desktop` logs
   `serving daemon_config.DaemonConfigService with no way to verify a session token — every call
   will be refused`, registers two services instead of the full set, and answers the dashboard's
   `GetAuthUrl` with `not_found` because `auth.AuthService` was never registered at all.
2. **The blast radius of the LiveKit secret is wrong.** `packages/tddy-daemon-auth/src/auth.rs:325`
   warns that a client holding it "could sign an access token for any GitHub user". Two backlog
   entries record the consequences: a client wanting to join a session room "has to hold
   `LIVEKIT_API_SECRET` and mint for itself — which is the fleet's session-token signing key, and
   therefore a real widening of the client trust surface"
   ([2026-08-15-session-worktree-sync-deliberate-gaps.md](../../../dev/todo/2026-08-15-session-worktree-sync-deliberate-gaps.md)),
   and `spawner.rs` passes the raw `--livekit-api-secret` on a child's command line where
   `/proc/<pid>/cmdline` exposes it
   ([2026-08-13-tokengenerator-generate-for-performs-no-authorization.md](../../../dev/todo/2026-08-13-tokengenerator-generate-for-performs-no-authorization.md)).
   Neither is closed here, but after this change both leak a room credential rather than the key
   that mints identities.
3. **There is no daemon identity to build on.** `#keyring` 6/9 authenticates which peers may receive
   vault state. Without a per-daemon key there is nothing to authenticate *as*.

## Proposed Changes

### What's Changing

**Token format — `v1` → `v2`** (`packages/tddy-github/src/session_token.rs`, 370 lines, one `verify`
entry point at `:171`). The payload gains a **key id** naming the signing daemon; the tag becomes an
Ed25519 signature over the same signed region. `v1` tokens are **rejected**, not accepted for a
migration window — a stale token in a browser fails and the client re-logs in.

**Key material** (`packages/tddy-daemon-auth`). A keypair is generated on first boot into
`auth_storage`, written through `tddy_core::atomic_file::write_atomic_with_mode` at mode `0600` —
the mode-aware variant, which sets the swap file's mode *before* writing rather than copying it from
a target that does not yet exist. An existing key is loaded, never regenerated. The public half is
exposed as SPKI DER plus a fingerprint, matching the shape
`packages/tddy-host-service/src/host_keypair.rs` already publishes.

**Key distribution — a port, not a dependency.** Verifying a peer's token needs that peer's public
key, and peers already exchange published metadata over the LiveKit common room. But
`packages/tddy-daemon-livekit/tests/dependency_boundary_unit.rs` walks the manifest closure and
fails if `tddy-daemon-auth` lands on `tddy-daemon-livekit`'s dependency path. So distribution
crosses the boundary exactly as minting already does: **auth owns a key-directory port;
`tddy-daemon-livekit` implements it.** `SessionTokenMinter` is the precedent being copied, and the
existing boundary test must still pass unchanged.

> **A cheaper-looking alternative exists, and this PRD rejects it.** `tddy-daemon-auth` already
> declares `livekit = "0.7"` and `tddy-livekit` itself, and `oauth_loopback_tunnel.rs:11,21` already
> drives a `Room` and an `RpcClient` from inside the auth crate. So auth *could* publish and read
> keys against the room directly, with no port and no new wiring — and the boundary test would stay
> green, because it constrains the opposite direction. The reason not to: the room is one transport,
> and a desktop or single-daemon deployment has no room at all. A port keeps "where do I get a
> peer's public key" answerable without one, which is exactly the coupling this node exists to
> remove. Taking the shortcut would re-create it in a new place.

**Configuration.** `livekit.api_secret` stops being read by the auth crate. `build_auth_entries` no
longer requires a `livekit` block to return a working signer, so a daemon with no `livekit:` at all
serves its token-gated services.

**Two backlog entries close with this node:**

- [`2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md`](../../../dev/todo/2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md)
  — and it is not optional. The test helper at `session_token.rs:232` flips the signature's final
  base64url character `'A' ↔ 'B'`. A 32-byte HMAC tag encodes to 43 characters whose last carries
  must-be-zero bits, so the flip produces a non-canonical token about 1 run in 64. An **Ed25519
  signature is 64 bytes** → 86 characters, whose final character has **4** must-be-zero bits, so
  only `A`, `Q`, `g` and `w` are legal there and the untampered tag ends in `'A'` roughly **1 time
  in 4**. Shipping `v2` without touching the helper escalates a 1.5% flake to ~25%.
- [`2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md`](../../../dev/todo/2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md)
  — the entry asks for a decision it could not make when the directory held only a token file. A
  private signing key is the fact that settles it: the daemon **warns once at startup** when
  `auth_storage` is more permissive than `0700`, rather than silently re-imposing a mode over an
  operator's deliberate `chmod`.

### What's Staying the Same

- **`livekit.api_secret` keeps signing LiveKit room JWTs.** It is not removed from configuration and
  its LiveKit role is untouched; it simply stops doing a second job.
- **The two-token access/refresh model**, the 5-minute and 7-day sliding TTLs, the `kind` checks in
  both directions, and client-side-only logout — all unchanged. This node changes *how a token is
  signed and verified*, not what a session is.
- **The GitHub OAuth flow**, `users:` mapping, and the `github:` requirement. Those are
  `#keyring` 2/9's; a daemon still needs a `github:` block after this node.
- **`FileGitHubTokenStore`** and its plaintext `github-tokens.json`. Replaced by `#keyring` 3/9.
- **The dependency-boundary tests** on both crates, which must pass unchanged — they are the
  guardrail this node is wired against, not something it relaxes.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-github` | `session_token.rs` — `v2` format, Ed25519 sign/verify, key id; the flaky helper at `:232`. Also `auth_service.rs:366`, whose test is literally named `a_token_minted_by_one_daemon_is_authenticated_by_another_sharing_the_secret` — the invariant this node replaces, so the test is rewritten rather than deleted |
| `tddy-daemon-auth` | `auth.rs` — keypair load/generate, the `0600` write, the permissions warning, `build_auth_entries` no longer reading `livekit`; `local_token.rs` — same signer, unchanged behaviour |
| `tddy-daemon-livekit` | implements the key-directory port; publishes this daemon's public key to the common room; **no new crate dependency** |
| `tddy-daemon` | `runtime.rs` — wiring the keypair and the port |
| `tddy-daemon-kernel` | `config.rs` — `livekit` is no longer a prerequisite for auth |
| `tddy-coder` | `run.rs:1131` `build_auth_service_entry` — the CLI's own signer construction |

**New dependency**: `ed25519-dalek` — **approved 2026-09-19** under CLAUDE.md § ASK. `zeroize` is
recommended for the secret key but optional. Nothing else is added — `#keyring` 6/9's
wrap-to-recipient reuses the workspace's existing pinned `rsa`.

**Four signer consumers**, not one: `auth.rs` (GitHub login), `local_token.rs` (UDS-resolved OS
user), `session_room.rs` through the `SessionTokenMinter` port (room tokens), and the two acceptance
suites that construct signers from a `FLEET_SECRET` constant.

**Two packages analyzed for this node**, both of which had no `docs/code-issues/` directory before it
(analyzed 2026-09-19 against a green baseline of **112 passed, 0 failed** from
`./test -p tddy-github -p tddy-daemon-kernel` — scoped to these two packages, as the verification
policy requires). Five records opened; **none blocks this node**, and this node claims none of them:

| Record | Bearing on `#keyring` 1/9 |
|---|---|
| [`heavy-dependency-livekit-peer-forwarding`](../../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md) | ⚠ **During** — constrains where this node's key material may live (see below) |
| [`oversized-file-config`](../../../../packages/tddy-daemon-kernel/docs/code-issues/oversized-file-config.md) | ⚠ During — this node edits `config.rs` but must not split it |
| [`missing-tests-real-exchange-code`](../../../../packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md) | — Unrelated to n1; **`#keyring` 2/9 should close it before adding the device flow** |
| [`missing-tests-privilege-drop-resolve-pty-os-user`](../../../../packages/tddy-daemon-kernel/docs/code-issues/missing-tests-privilege-drop-resolve-pty-os-user.md) | — Unrelated |
| [`misplaced-tests-privilege-drop`](../../../../packages/tddy-daemon-kernel/docs/code-issues/misplaced-tests-privilege-drop.md) | — Unrelated |

**What the heavy-dependency record changes for this node.** `peer_forwarding.rs` is the sole consumer
of the LiveKit SDK in `tddy-daemon-kernel`, and it makes all 14 dependents carry `livekit = "0.7"` —
six of them for nothing. Two modules already record that as a constraint they had to design around,
because the SDK cannot enter `tddy-tools`' `--no-default-features` in-jail build. **So this node's
keypair and key-directory port must not land in `tddy-daemon-kernel`**, or they inherit that
unreachability. They land in `tddy-daemon-auth`, which is where this PRD already places them — the
record turns that from a preference into a constraint.

### User Impact

- **Breaking: every browser session ends.** `v1` tokens are rejected, so every signed-in client
  re-logs in once. Deliberate, and consistent with this repo's policy of breaking freely and
  migrating every consumer in the same change.
- **Breaking: configuration.** A deployment relying on `livekit.api_secret` to make authentication
  work gets authentication that works without it. Nothing an operator must do; a setting they no
  longer need to invent.
- **Fleet operators** get one new obligation: peers must be able to exchange public keys, which
  means a common room. A fleet that was mutually verifying tokens *was* already sharing a room.
- **Single-daemon and desktop deployments** are unaffected by distribution entirely — one daemon,
  one key, nothing to publish.

## Implementation Plan

1. ~~Approve `ed25519-dalek`~~ — **done 2026-09-19**.
2. `v2` token format in `session_token.rs`, with the flaky helper corrected in the same commit.
3. Keypair generation, load and the `0600` write in `tddy-daemon-auth`; the `auth_storage`
   permissions warning.
4. The key-directory port in `tddy-daemon-auth`, and its implementation in `tddy-daemon-livekit`
   publishing to and reading from the common room.
5. Rewire `build_auth_entries`, `local_token.rs`, `runtime.rs` and `run.rs` off `livekit.api_secret`.
6. Migrate the two acceptance suites from `FLEET_SECRET` to per-daemon keys.
7. Update `desktop.yaml.production` and the three docs that currently state the barrier.

**Verification is scoped** to the packages above (`./test -p tddy-github -p tddy-daemon-auth
-p tddy-daemon-livekit -p tddy-daemon`), and whole-workspace green comes from CI via
`scripts/ci-status.sh`, never from a local run.

## Acceptance Criteria

- [x] A daemon with **no `livekit:` block at all** completes a sign-in, and the token it issues
      resolves to the user who signed in ([session-auth.md](../session-auth.md)). Registration was
      never the barrier — measured, `auth.AuthService` is registered without `livekit:` today;
      the sign-in fails one step later at `ExchangeCode`, with
      `FailedPrecondition: "session token signing is not configured"`
- [x] A daemon generates its keypair on first boot, at mode `0600`, and **reuses** it on restart
- [x] A `v2` token minted by daemon A verifies on daemon B after B has seen A's published public key
      ([livekit-peer-discovery.md](../livekit-peer-discovery.md))
- [x] A `v2` token whose key id names a daemon B has **not** seen is rejected — no fallback
- [x] A `v1` token is rejected
- [x] `livekit.api_secret` still mints a working LiveKit room JWT
      ([auth-livekit-services.md](../auth-livekit-services.md))
- [x] Both crates' dependency-boundary tests pass **unchanged**
- [x] `verify_rejects_a_token_with_a_tampered_signature` passes 100 consecutive runs
- [x] The daemon warns once at startup when `auth_storage` is more permissive than `0700`
- [x] Access/refresh TTLs, `kind` enforcement in both directions, and logout behaviour are unchanged

## References

### Affected Features (Complete List)

- [Cross-daemon session authentication](../session-auth.md) — token format, signing key, verification
- [The identity boundary and the LiveKit service](../auth-livekit-services.md) — the shared-secret rule
- [LiveKit peer discovery](../livekit-peer-discovery.md) — forwarded-token trust
- [Tddy Desktop](../../desktop/tddy-desktop-tauri.md) — one of three install barriers removed
- [Daemon settings](../daemon-settings.md) — reachable without a LiveKit secret

### Stack

Root node of `#keyring` (9 nodes). Direct dependents: `#keyring` 2/9 `desktop-login` and
`#keyring` 3/9 `store`. `#keyring` 6/9 `sync` consumes the daemon identity this node establishes.

### Backlog

- ✅ [2026-08-02 — flaky tampered-signature test](../../../dev/todo/2026-08-02-verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64.md)
- ✅ [2026-09-10 — `ensure_owner_only_dir` and `auth_storage`](../../../dev/todo/2026-09-10-ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage.md)
- ⚠ [2026-09-18 — a desktop install configures no identity](../../../dev/todo/2026-09-18-desktop-install-configures-no-identity.md) — claimed by `#keyring` 2/9
