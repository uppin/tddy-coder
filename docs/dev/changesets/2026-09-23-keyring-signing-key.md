# 2026-09-23 — Each daemon signs session tokens with an Ed25519 key of its own

**Type:** Architecture

`#keyring` 1/9, the stack's root node — PR [#508](https://github.com/uppin/tddy-coder/pull/508),
base `master`. Direct dependents: `#keyring` 2/9 [#509](https://github.com/uppin/tddy-coder/pull/509)
(desktop login), 3/9 [#510](https://github.com/uppin/tddy-coder/pull/510) (the credential store),
and 6/9 [#513](https://github.com/uppin/tddy-coder/pull/513) (peer sync), which authenticates peers
*as* the identity this node establishes.

`config.livekit.api_secret` signed **both** LiveKit room JWTs and session tokens, so a LiveKit
credential gated all authentication and anyone holding it could sign an access token for any
GitHub login fleet-wide. Each daemon now generates an **Ed25519 keypair** on first boot
(`signing_key.pem`, mode `0600`, in `auth_storage` or `<tddy_data_dir>/auth`) and signs **`v2`**
session tokens with it; a token names its signer's key id, and a daemon verifying a peer's token
resolves that id through a **`KeyDirectory` port** that `tddy-daemon-auth` owns and `tddy-daemon`
implements over the common-room advertisement. `livekit.api_secret` signs room JWTs and nothing
else. A daemon with no `livekit:` block completes a sign-in.

**Breaking, with no fallback.** `v1` (HMAC-SHA256) tokens are refused as `UnsupportedVersion`, so
every signed-in client signs in once more.

Where the end state is documented:

- [session-auth.md](../../ft/daemon/session-auth.md) — the token model, key location, security posture
- [auth-livekit-services.md § One key signs one thing](../../ft/daemon/auth-livekit-services.md)
- [livekit-peer-discovery.md § Trust model](../../ft/daemon/livekit-peer-discovery.md) — who may be taken for a daemon, content-addressed key ids, the revocation trade-off
- [`tddy-github/docs/session-token.md`](../../../packages/tddy-github/docs/session-token.md) — the `v2` wire format
- [`tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — `DaemonSigningKey`, `KeyDirectory`, `verify_now`, `SessionTokens`
- [`tddy-daemon-livekit/docs/livekit-service.md`](../../../packages/tddy-daemon-livekit/docs/livekit-service.md) — `AdvertisedSigningKey`, the identity rule
- [`tddy-daemon/docs/daemon-endpoint.md`](../../../packages/tddy-daemon/docs/daemon-endpoint.md) — `runtime::build`'s signing identity, `CommonRoomKeyDirectory`
- [`tddy-daemon-kernel/docs/daemon-kernel.md`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — why neither the key nor the identity rule lives in the kernel

## What changed, by package

| Package | Change |
|---|---|
| `tddy-github` | `session_token_v2.rs` — `v2`, Ed25519, `KeyId` derived from the SPKI DER, `SessionTokenAuthority`; `session_token.rs` (`v1`) deleted, `hmac`/`subtle` dropped; `AuthServiceImpl::new_signed` takes an authority |
| `tddy-daemon-auth` | `signing_key.rs` (new) — `DaemonSigningKey`, `load_signing_key`, `KeyDirectory`, `StandaloneKeyDirectory`, `DirectorySessionTokenVerifier`, `SessionTokens`; `build_auth_entries_with` off `livekit`; the `auth_storage` posture warning; `AUTH_LOG_TARGET` |
| `tddy-daemon-livekit` | `AdvertisedSigningKey` on the advertisement, `peer_signing_public_keys` / `signing_public_keys_for` (every candidate, undecoded); discovery refuses every non-daemon identity |
| `tddy-daemon` | `common_room_key_directory.rs` (new); `runtime.rs` builds the one `SessionTokens`; direct `ed25519-dalek` dependency |
| `tddy-service` | `participant_identity.rs` (new) — `may_be_daemon_discovery_identity` and `NON_DAEMON_IDENTITY_PREFIXES`, read by both discovery and `token.TokenService`, which now refuses every identity it allows |
| `tddy-daemon-kernel` | two `config.rs` doc comments; `daemon_identity` re-exports `SPLIT_AGENT_IDENTITY_PREFIX` from `tddy-service` |
| `tddy-session-lifecycle` | `split_session.rs` and the connection service verify callers and mint agents' credentials through `SessionTokens`; the exec-tool refusal names the unlearned signing key |
| `tddy-daemon-rpc` | tests only: the suites `#carve` 11/12 (#520) moved or split out of `tddy-session-lifecycle` — cross-daemon tokens, the sandboxed-codebase lifecycle, placement and seatbelt exec-tool suites — give each daemon a signing identity of its own instead of `livekit.api_secret`; `tddy-daemon-auth` becomes a dev-dependency |
| `tddy-screen-sharing` | the bridge's `screenshare-host-` prefix is one constant shared with the identity rule |
| `tddy-worktree-service`, `tddy-remote-git-repo`, `tddy-vm-testkit`, `tddy-desktop`, `tddy-web`, `tddy-connectrpc`, `tddy-e2e`, `tddy-rust-typescript-tests` | stale comments, `desktop.yaml.production`, regenerated `auth_pb.ts`, and fixtures asking the mint for `web-` identities |

`tddy-coder` was planned and needed no change: `build_auth_service_entry` gates on the GitHub OAuth
app's credentials, and the CLI never held a session-token signer.

**New dependency:** `ed25519-dalek` (developer-approved 2026-09-19), in `tddy-github`,
`tddy-daemon-auth` and `tddy-daemon`. No crate entered `Cargo.lock`.

## Decisions worth keeping

- **The adapter lives in `tddy-daemon`**, not `tddy-daemon-livekit`: that crate's
  `dependency_boundary_unit` forbids reaching `tddy-daemon-auth`, whose trait `KeyDirectory` is.
  Both dependency-boundary suites pass **unchanged**. The cheaper route — auth reading keys off the
  room directly — was rejected: a desktop or single-daemon deployment has no room.
- **`KeyDirectory` stays async and resolves only** (developer decision). `verify_now` polls once;
  `publish` was dropped for having no production caller.
- **A key file other accounts can read is refused, not repaired**; the `auth_storage` directory is
  only warned about, once.
- **First boot hard-links** the staged key into place so concurrent boots on one data dir keep one key.
- **Key location refuses rather than guesses** when neither `auth_storage` nor `tddy_data_dir` is set.
- **The mint/discovery identity rule** closed a forgery this node would otherwise have opened: under
  the first cut, any signed-in web user could be minted a bare common-room identity with
  `can_update_own_metadata`, advertise a keypair, and forge tokens for any login.
  `common_room_key_trust_acceptance.rs` exercises that attack.
- **Learned keys survive a reconnect**, trading revocation for availability — recorded in the backlog.

**Left as they are, with reasons.** `AdvertisedSigningKey: Default` (≈15 test call sites; documented
as not a production state). The PEM read is not `Zeroizing` (no new dependency). `tddy-daemon`'s
direct `ed25519-dalek` edge and the widened `unbundle_endpoint` module list.
`DaemonSigningKey::key_id()` (owned) vs `SessionTokenSigner::key_id()` (borrowed), kept to spare
eight dependents' rebases. `tddy-daemon-livekit/tests/dependency_boundary_unit.rs:67`'s doc comment
still says `api_secret` signs session tokens — the file had to stay byte-unchanged in this node, and
correcting it is left to whoever next touches it. Validate-tests nits not taken: an `exp == now`
boundary test, a `v3.` version test, `v1` rejection at the resolver level, a regression test for an
advertisement without signing fields, and one shared in-memory `KeyDirectory` fake instead of three.
`token.TokenService` still mints `server…`, `split-agent-…` and `remote-git-…` identities to an
authenticated caller (pre-existing; its refusal text now names every accepted prefix).

## Backlog entries resolved (deleted from `docs/dev/todo/`)

- **`verify_rejects_a_token_with_a_tampered_signature` is flaky ~1-in-64** (2026-08-02,
  `verify-rejects-a-token-with-a-tampered-signature-is-flaky-1-in-64`). The helper flipped the
  signature's last base64url character, which under Ed25519 would have been a ~25% flake. It now
  decodes the signature, flips `signature[0] ^= 0x01` and re-encodes (`session_token_v2.rs`);
  100/100 consecutive runs green.
- **`ensure_owner_only_dir` no longer re-tightens an existing `auth_storage`** (2026-09-10,
  `ensure-owner-only-dir-no-longer-re-tightens-an-existing-auth-storage`). Decided as the entry
  asked: warn once at startup when the directory is more permissive than `0700`, never re-impose
  the mode — `auth_storage_posture_warning_acceptance.rs` (one record for `0755`, none for `0700`).
  The entry's other half (parents created `0700`) is a documented, deliberate consequence in
  `auth-service.md` § Secrets at rest, not an open defect.

Filed by this node and kept open: `2026-09-23-a-learned-peer-signing-key-is-never-revoked` and
`2026-09-23-vm-testkit-guest-livekit-block-is-stale` (backs the `TODO(keyring)` in
`tddy-vm-testkit/src/test_host_vm.rs`). Premises corrected, entries kept open:
`2026-08-15-session-worktree-sync-deliberate-gaps`, `2026-08-03-tddy-supervisor-vm-backed-acceptance-test`,
`2026-07-04-tddy-github-tddy-daemon` (its telegram-signer bullet assumed an HMAC session signer).

## Code issues — re-measured at wrap

Structural scan (lines, nesting by indentation, `if`/`match` count, `return`/`?` count) run on
`origin/master` (`4e260d7f`, after #520 `#carve` 11/12) and on this branch; production lines counted to the first `#[cfg(test)]`.
The scan reproduces each record's first-detection numbers on master.

| Record | Before → after | Verdict |
|---|---|---|
| `tddy-daemon-auth` `complexity-auth-build-auth-entries` | `build_auth_entries` 104 lines / 6 branches / 3 exits → 12 lines; the body moved to `build_auth_entries_with`, 92 / 6 / 3 | **Moved + partially fixed** — renamed `complexity-auth-build-auth-entries-with`; 92 > 60 remains |
| `tddy-daemon-livekit` `heavy-params-connect-common-room-publish-metadata` | 7 params / 65 lines / nesting 6 → 5 / 56 / 6 | **Partially fixed** — params and length within budget; nesting remains |
| `tddy-session-lifecycle` `oversized-file-svc-spawn-split-agent` | 520 → 505 production lines | **Partially fixed** — 5 over budget |
| `tddy-daemon` `complexity-runtime-build` | `build` 833 → 878 lines, branches 18 → 20, exits 13 → 14 | **Regressed** |
| `tddy-session-lifecycle` `complexity-svc-spawn-split-agent-spawn-split-agent` | 251 → 254 lines, nesting and params unchanged | **Regressed** |
| `tddy-session-lifecycle` `oversized-file-connection-service` | total 1,647 → 1,652 (~1,634 → ~1,639 production; all five lines production) | **Regressed** |
| `tddy-coder` `complexity-run-build-auth-service-entry` | 65 lines / nesting 6 / 0 exits, identical | **Unchanged** — named by the plan, not touched |
| `oversized-file-{runtime, config, livekit-peer-discovery, split-session}` | 1,513 → 1,562 · 1,467 → 1,470 · 1,565 → 1,638 · 651 → 647 | Rows written by the refactor pass, confirmed |
| `tddy-session-lifecycle` `oversized-file-svc-resolve-os-user` | 538 → 539 when opened (on `77187dbe`); 358 → 359 after rebasing onto #520, which moved `authorize_exec_tool_caller` and its neighbours out of the `DaemonSessionHost` impl | **Clean — record deleted** (359 < 500) |
| `oversized-file-{session-room, remote-git-service}` | 2,874 · 1,002 → 1,004 | Opened by the refactor pass, confirmed |

One record was closed — `oversized-file-svc-resolve-os-user`, by the rebase rather than by this node. Records in files this node touched but in functions it did not —
`complexity-session-room-{run,git-output,take-c-quoted}`,
`complexity-svc-spawn-split-agent-delete-paired-codebase-session`,
`complexity-svc-resume-claude-cli-session-resume-claude-cli-session` — measure byte-identical and
were left alone. None of the six stack-shared oversized files was split: dependents #509–#513 touch
them.
