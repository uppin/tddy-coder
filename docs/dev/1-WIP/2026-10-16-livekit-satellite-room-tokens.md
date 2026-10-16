# Changeset: Room tokens through an extension port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #626
**Type**: Refactor (architecture)
**Stack**: #satellite 4/13. Base: `feature/satellite/session-rooms` (#625).

## Initial Discovery

[2026-10-16-livekit-satellite-room-tokens-initial-discovery.md](./2026-10-16-livekit-satellite-room-tokens-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-08-13-tokengenerator-generate-for-performs-no-authorization.md](../todo/2026-08-13-tokengenerator-generate-for-performs-no-authorization.md) | ⚠ DURING | The port mints for any room and identity it is asked for, exactly as `generate_for` does. Authorization stays with the callers (`TokenServiceImpl`'s gates). The port must not be handed to an unauthenticated surface; recorded in the port's doc. |
| [2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md](../todo/2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation.md) | ⚠ DURING | Removes the key and secret re-reads in screen sharing and daemon-auth. Narrowed, not closed. |

## Responsibility

Room-access-token minting as a port. Owns `RoomTokenMinter` and `NoRoomTokens` in
`tddy-daemon-extensions`, the plugin's config-built minter (`room_tokens` module), screen sharing's
and daemon-auth's switch onto the port, and the daemon wiring that builds the minter once. Strikes
`tddy-screen-sharing`.

## Boundaries

- Does not move the minting in `tddy-coder` (node 8), `tddy-session-sync` and `tddy-terminal-rpc`
  (node 7), `tddy-cli-sessions` (node 6), or `tddy-session-agents` and `tddy-session-split`
  (node 11). Those keep calling `tddy_livekit::TokenGenerator` until their node.
- Does not touch `SessionTokenMinter` (the plugin's port for a split agent's session token), which
  is node 11.
- Does not strike `tddy-daemon-auth`; its `oauth_loopback_tunnel.rs` still uses `livekit::Room` and
  `tddy_livekit::RpcClient` (node 10).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | `tddy-daemon-extensions`; ratchet list | Adds `room_tokens.rs`; strikes one entry | Touch `SessionReach` or the status board |
| #624 peer-forwarding, #625 session-rooms | Nothing consumed (line predecessors) | — | Touch `PeerForwarder`, `SessionRooms` |

**As published (wave 2):** the surface is additive so no existing test is broken by a stub: the
port, `room_token_minter`, `MinterTokenProvider`, and screen sharing's `with_room_tokens` builder plus
a minter parameter threaded through `prepare_bridge` / `build_bridge_spawn_config` (both private, so
the bridge test lives in that module's `tests`, not `tests/bridge_room_token.rs`). Green switches
`build_token_service_entry`, `LiveKitTokenServiceImpl` and the runtime onto the minter. The ratchet
assertions additionally wait on #623's `WorkspaceGraph`.

## Draft PR contract

```rust
// tddy-daemon-extensions/src/room_tokens.rs
/// Mints LiveKit-style room access tokens. Performs no authorization: hand it only to code that
/// has already decided the caller may have the token.
pub trait RoomTokenMinter: Send + Sync {
    fn mint(&self, room: &str, identity: &str, ttl: Duration) -> Result<String, Status>;
}
/// No integration: every mint is a failed precondition.
pub struct NoRoomTokens;

// tddy-daemon-livekit/src/room_tokens.rs
/// The minter the `livekit:` block supports: a real one when url, api_key and api_secret are all
/// set; otherwise one whose every mint fails naming the missing fields.
pub fn room_token_minter(livekit: Option<&LiveKitConfig>) -> Arc<dyn RoomTokenMinter>;

// tddy-daemon-auth
pub fn build_token_service_entry(config: &DaemonConfig, user_resolver: Option<&SessionUserResolver>,
                                 minter: Arc<dyn RoomTokenMinter>) -> Option<ServiceEntry>;
pub struct MinterTokenProvider { .. }       // impl tddy_service::TokenProvider over a minter
// LiveKitTokenServiceImpl gains the minter (constructor parameter)

// tddy-screen-sharing: the service is built with `Arc<dyn RoomTokenMinter>`
```

Bodies are `todo!()` until green; the failing tests below are published with the surface.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes; every test drives this node's own port and adapters.
**Concurrent with:** nodes 2, 3 and 5–9
**Blocks:** node 10 (#632 — daemon-auth is struck only once its minting is off `TokenGenerator`), node 11 (#633 — the clone's self-mint) and node 12 (#634 — the split agent's join token)

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

`tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-daemon-auth`, `tddy-screen-sharing`,
`tddy-daemon` (wiring), `tddy-session-lifecycle` (re-export of `auth::build_token_service_entry`).

## Summary

The daemon decides once whether it can mint LiveKit tokens; everything else asks the port.

## Scope

- [ ] Port + null object
- [ ] Plugin `room_token_minter`, naming missing fields
- [ ] Screen sharing on the port; manifest trimmed
- [ ] daemon-auth token service and LiveKit token service on the port; `LiveKitTokenProvider` replaced by `MinterTokenProvider`
- [ ] Runtime builds the minter once
- [ ] Ratchet: strike `tddy-screen-sharing`

## Technical Changes

### State A
- `tddy-service/src/token_service.rs:13-18` `TokenProvider { generate_token(room, identity) ->
  Result<String,String>; ttl_seconds() }` already decouples `TokenServiceImpl` from LiveKit.
- `tddy-daemon-auth/src/token_provider.rs` `LiveKitTokenProvider(Arc<tddy_livekit::TokenGenerator>)`.
- `auth.rs:319-345` `build_token_service_entry(config, user_resolver)` reads `api_key`/`api_secret`
  and builds a generator with placeholder room and identity. It is called at `auth.rs:1100,1231,1254`
  and `tddy-daemon/src/runtime.rs:758` (via the lifecycle re-export).
- `auth.rs:455-550` `LiveKitTokenServiceImpl::mint_live_kit_token` builds a generator per call with
  `MINTED_ROOM_TOKEN_TTL` (`:437`, 3600 s).
- `tddy-screen-sharing/src/screen_sharing_service.rs:477-515` `build_bridge_spawn_config` reads url,
  key and secret, then mints with `DEFAULT_LIVEKIT_JWT_TTL_SECS`.
- `tddy-livekit/src/token.rs:18-50` `TokenGenerator::{new, generate, generate_for}`.

### State B
One minter is built in runtime from `config.livekit` and passed to the auth builders and screen
sharing. Screen sharing still reads the *URL* from config (it is not a credential), and mints
through the port.

### Delta
| Package | Change |
|---|---|
| `tddy-daemon-extensions` | `src/room_tokens.rs`, `lib.rs` |
| `tddy-daemon-livekit` | `src/room_tokens.rs` (new), `lib.rs` |
| `tddy-daemon-auth` | `auth.rs`, `token_provider.rs` → `MinterTokenProvider` |
| `tddy-screen-sharing` | `screen_sharing_service.rs`; `Cargo.toml` −`tddy-livekit` +`tddy-daemon-extensions` |
| `tddy-daemon` | `runtime.rs` |
| ratchet | strike `tddy-screen-sharing` |

## Implementation Milestones
- [ ] M1 Port, null object, plugin minter with verify tests
- [ ] M2 Screen sharing and daemon-auth switched
- [ ] M3 Manifest trimmed; ratchet green

## Testing Plan

The plugin's minter is verified by decoding the token it mints with LiveKit's own verifier, keyed by
the configured secret (`livekit-api`, which the plugin may name). Consumers are tested with a
recording fake minter: what they pass in, and that they hand on what comes back.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_minted_token_grants_the_requested_room_to_the_requested_identity` | `packages/tddy-daemon-livekit/tests/room_tokens.rs` (new) | Verified with the configured secret: `video.room`, `sub` identity, TTL |
| `a_block_without_its_secret_mints_nothing_and_names_the_field` | same | `FailedPrecondition` naming `api_secret` |
| `a_daemon_without_livekit_mints_nothing` | same | `room_token_minter(None)` refuses |
| `the_bridge_is_handed_the_token_minted_for_its_room_and_identity` | `packages/tddy-screen-sharing/src/screen_sharing_service.rs` (`tests` module; the function under test is private) | Recording minter sees (room, bridge identity, default TTL); the spawn config carries its token |
| `the_token_service_mints_through_the_injected_minter_for_the_requested_room_and_identity` | `packages/tddy-daemon-auth/tests/token_minting_port.rs` (new) | `GenerateToken` (authenticated) returns the minter's token; TTL from the minter adapter |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Entry struck; both ratchet tests pass |

Unit (`tddy-daemon-extensions/src/room_tokens.rs`): `with_nothing_wired_every_mint_is_a_failed_precondition`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **A new port beside `tddy_service::TokenProvider`, not a replacement.** `TokenProvider` is the
  token *service's* port, with a fixed TTL and string errors. The daemon needs per-call TTLs
  (`DEFAULT` and `MINTED_ROOM_TOKEN_TTL`) and `Status` errors. An adapter keeps both honest.
- **The minter names missing fields.** It replaces screen sharing's three field-specific errors with
  one source of that wording.
- **`build_bridge_spawn_config` must become reachable from a test** (today it is a private `fn`).
  Prefer testing it through the service's public spawn path. If that needs a running VNC target,
  expose `build_bridge_spawn_config` as `pub(crate)` and test it in-crate. Decided during wave 2.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-16-livekit-satellite-room-tokens-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
