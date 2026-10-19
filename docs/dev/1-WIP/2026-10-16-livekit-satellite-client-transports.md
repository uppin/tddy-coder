# Changeset: Client room transport through a port

**Date**: 2026-10-16
**Status**: 🚧 In Progress
**PR**: #629
**Type**: Refactor (architecture)
**Stack**: #satellite 7/13. Base: `feature/satellite/terminal-bridge` (#628).

## Initial Discovery

[2026-10-16-livekit-satellite-client-transports-initial-discovery.md](./2026-10-16-livekit-satellite-client-transports-initial-discovery.md)

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md](../todo/2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md) | ⚠ DURING (kept) | Every client joins through one `RoomConnector`, which retires the three `connect_client` call sites the entry cites. Both defects the entry records — the two near-duplicate streamer crates and the ten test files with their own connect-and-wait — are untouched, so the entry is **not** closed and stays (verdict downgraded from ✅ at /pr-wrap, 2026-10-19). |
| [2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md](../todo/2026-10-01-session-tool-client-repeats-its-transport-selection-three-times.md) | ⚠ DURING | The LiveKit arm changes in all three copies; de-duplication stays out of scope. |
| `packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md` | ⚠ DURING | The LiveKit arm shrinks (connect + cache move behind the port); no new code in `lib.rs`. |
| `packages/tddy-session-sync/docs/code-issues/oversized-file-attach.md` | ⚠ DURING | `attach.rs` loses its token and connect code. |

## Responsibility

The client side of "join a room and talk in it". Owns the new crate `tddy-room-client`
(`RoomJoin`, `RoomAuth`, `RoomConnector`, `RoomSession`, `RoomDuplex`, `RoomConnectError`), the
plugin's `LiveKitRoomConnector` in `tddy-livekit`, the four libraries' switch to it, and the three
binaries' wiring. Strikes `tddy-session-sync`, `tddy-remote-git-repo`, `tddy-terminal-rpc` and
`tddy-session-tool-client`.

## Boundaries

- Does not touch the daemon side (rooms, bridges, forwarding).
- Does not touch `tddy-coder` (node 8). Node 8 may *reuse* this port, which is its decision.
- Does not de-duplicate tool-client's transport selection.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #623 `satellite-1-extension-ports` | `open_session_room` (session-sync) and `open_session_room_over_http` (pty-relay), which a client calls before joining; the ratchet | Joins after those calls, through the new port; strikes four entries | Change those two functions |
| #624–#628 | Nothing consumed | — | — |

**As published (wave 2):** the `tddy-room-client` crate (port, `NoRoomTransport`, and the
`test-support` `in_memory` connector — a real fake, it is test infrastructure), `LiveKitRoomConnector`
in `tddy-livekit` (todo!()), `tddy_session_sync::join_session_room` and `tddy_remote_git_repo::relay::run_with`
(todo!()), with failing tests for the port and for session-sync's join. Additive by design: the old
entry points stay until green so no existing suite breaks on a stub. **Not yet published:** the
connector entry points and tests for `tddy-terminal-rpc` (pty-relay) and `tddy-session-tool-client`
(room cache), and remote-git's relay test — they land in green with the switch. The ratchet
assertions wait on #623's `WorkspaceGraph`; the bin-crate decision (Decisions) is still open.

**As built (green, 2026-10-19):** the port crate, `LiveKitRoomConnector`, and the four libraries on the
port with their binaries wiring the connector. Where the build departs from the contract above:
`tddy_remote_git_repo::relay::run` now takes the connector (the contract's `run_with` was folded into it,
since the old entry point had to go) and its stdio pump is `relay_over_duplex`, the seam the relay test
drives; `tddy_session_sync::attach(credentials, connector)` and `AttachedSession` carry an
`Arc<dyn RoomSession>` and plain `Vec<u8>` payloads (`AttachError::Token` is gone — a mint failure is
now a `RoomConnectError::Room`); `tddy_terminal_rpc::pty_relay::run_pty_relay(config, connector)`; the
tool client takes its connector from `install_room_connector`, a process-wide install the binary makes
once (its entry points — `dispatch_session_tool`, the roster and the conversation calls — are called from
too many places for a parameter, and changing them is out of scope here). `RoomConnectError` gained
`Display`. `RoomAuth::Token` still leaves `RoomJoin.room`/`identity` carrying nothing the token does not
(tool-client passes the destination's room and an empty identity, as its test pins); aligning with
#630's credentials-variant shape was left, because it rewrites every `RoomJoin` in four red test files for
no behaviour.

**Red pass (2026-10-17):** added `join_terminal_room` + `tests/pty_relay_room_join.rs` (2 tests),
`room_session_via` + `tests/room_transport_cache.rs`, and `in_memory::a_session_with_one_duplex`.
`the_relay_pipes_stdio_through_a_duplex_to_the_remote_git_service` was written in green, against the
relay's `relay_over_duplex` seam. **For green:**
with `RoomAuth::Token`, `RoomJoin.identity`/`room` carry nothing the token does not; align with #630's
shape (room and identity inside the credentials variant) when the port is implemented.

## Draft PR contract

```rust
// packages/tddy-room-client/src/lib.rs  (new crate: tddy-rpc + tokio + async-trait only)
pub enum RoomAuth { Credentials { api_key: String, api_secret: String, ttl: Duration }, Token(String) }
pub struct RoomJoin { pub url: String, pub room: String, pub identity: String, pub auth: RoomAuth }
pub enum RoomConnectError { Room(String), PeerAbsent { identity: String, waited: Duration } }
#[async_trait::async_trait]
pub trait RoomConnector: Send + Sync {
    async fn connect(&self, join: &RoomJoin, peer_identity: &str, wait: Duration)
        -> Result<Arc<dyn RoomSession>, RoomConnectError>;
}
pub trait RoomSession: Send + Sync {
    fn rpc(&self) -> Arc<dyn tddy_rpc::RpcClientTransport>;
    fn open_duplex(&self, service: &str, method: &str) -> Result<RoomDuplex, tddy_rpc::Status>;
    fn subscribe(&self, topic: &str) -> tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>;
    fn peer_present(&self) -> bool;
}
/// A live bidi stream: send frames as they come, receive replies as they arrive.
pub struct RoomDuplex { pub outbound: mpsc::Sender<Vec<u8>>, pub inbound: mpsc::Receiver<Result<Vec<u8>, Status>> }
/// No transport wired: every connect fails `RoomConnectError::Room("no room transport")`.
pub struct NoRoomTransport;

// tddy-livekit/src/room_connector.rs
pub struct LiveKitRoomConnector;
impl RoomConnector for LiveKitRoomConnector { .. }
```

Library entry points gain a connector parameter, for example `tddy_session_sync::attach(credentials,
connector: Arc<dyn RoomConnector>)`. The exact parameter for each library is published with the
surface. Bodies are `todo!()` until green.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes; library tests use an in-memory connector, and binaries keep the
existing testkit suites.
**Concurrent with:** nodes 2–6, 8 and 9
**Blocks:** none (node 8 may reuse the crate; if it does, record the edge n7 → n8 there)

    n1 → n2..n9   n2, n4 → n10   n2, n3, n4, n7, n10 → n11   n4, n10 → n12   n11, n12 → n13

## Affected Packages

New `tddy-room-client`; `tddy-livekit`, `tddy-session-sync`, `tddy-remote-git-repo`,
`tddy-terminal-rpc`, `tddy-session-tool-client`, `tddy-discovery`, `tddy-tools`.

## Summary

Client libraries talk to "a room" and only their binaries know it is LiveKit.

## Scope

- [x] `tddy-room-client` crate (port + null object + in-memory test connector behind a `test-support` feature)
- [x] `LiveKitRoomConnector` in `tddy-livekit`
- [x] session-sync `attach` / `sync` on the port; binary wires the connector
- [x] remote-git-repo relay on the port (live duplex); binary wires it
- [x] terminal-rpc pty-relay LiveKit mode on the port; `livekit` feature removed
- [x] tool-client LiveKit transport + room cache on the port; `livekit` feature removed; discovery's feature removed
- [x] tddy-tools wires the connector behind its `livekit` feature
- [x] Ratchet: strike four entries

## Technical Changes

### State A
- `tddy-session-sync/src/attach.rs:27-29` (`BroadcastMessage`, `connect_client`, `ConnectError`,
  `ConnectedClient`, `BroadcastChannel`, `TokenGenerator`, `DEFAULT_LIVEKIT_JWT_TTL_SECS`);
  `sync.rs:25` (`RpcClient`). Manifest `:16` `tddy-livekit`. lib + bin.
- `tddy-remote-git-repo/src/relay.rs:9` (`connect_client`, `ConnectError`), `:21`
  (`chunking::MAX_CHUNK_FRAME_BYTES`), `:139` (`BidiStreamSender`). Manifest `:16`. lib + bin.
- `tddy-terminal-rpc/src/pty_relay.rs:306,318,329` (`TokenGenerator`, `connect_client`), all behind
  the optional `livekit` feature (`Cargo.toml:24-26,35-36`).
- `tddy-session-tool-client/src/lib.rs:466,1024` (`RpcClient`), `:535-582` (`livekit::prelude`,
  `connect_client`, `livekit::Room`, `BroadcastChannel`), optional feature `Cargo.toml:27-37`.
- `tddy-discovery` has a feature only (`Cargo.toml:40`); `tddy-tools` `default = ["livekit"]`
  (`Cargo.toml:78-88`).
- `tddy-livekit/src/client_connect.rs:19-60`: `ConnectError { Room, ParticipantAbsent }`,
  `ConnectedClient { room, client }`, `connect_client(url, token, target_identity, wait)`.
- `tddy-rpc/src/transport.rs:18` `RpcClientTransport` (unary, server, client, bidi-with-known-requests).

### State B
Each library holds an `Arc<dyn RoomConnector>` given by its binary and works through `RoomSession`.
`tddy-livekit`'s connector owns minting, joining, the participant wait, broadcasts and duplex streams.

### Delta
As listed in Scope. Plus `Cargo.toml` changes: the four libraries −`tddy-livekit`/−`livekit`,
+`tddy-room-client`; their binaries +`tddy-livekit` (bin-only via `[[bin]]` +
`required-features`/dependency placement). See Decisions.

## Implementation Milestones
- [x] M1 Port crate + in-memory connector; plugin connector over `connect_client`
- [x] M2 session-sync and remote-git-repo switched (testkit suites green through the bins)
- [x] M3 terminal-rpc and tool-client switched; features removed
- [x] M4 Manifests; ratchet green

## Testing Plan

Library behaviour is tested against an in-memory `RoomConnector`: the join it requests, the RPCs it
makes, and what it does on `PeerAbsent`. The LiveKit connector keeps coverage through the existing
testkit-backed suites of each tool, which run through the binaries' wiring.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_mirror_joins_the_session_room_under_its_own_identity_and_waits_for_the_daemon` | `packages/tddy-session-sync/tests/attach_room_join.rs` (new) | `attach` asks the connector for `session-{id}`, identity `syncer-…`, peer `daemon-{instance}`, credentials auth |
| `a_daemon_that_never_arrives_is_reported_absent` | same | `PeerAbsent` → `AttachError::DaemonAbsent { identity, room, waited }` |
| `the_relay_pipes_stdio_through_a_duplex_to_the_remote_git_service` | `packages/tddy-remote-git-repo/tests/relay_over_room.rs` (new) | Bytes written to the relay's stdin arrive on the duplex's outbound side, and inbound frames reach stdout (through `relay_over_duplex`; `relay::run`'s own join is covered by the testkit suites through the binary) |
| `pty_relay_joins_the_room_it_was_told_as_its_own_identity_and_waits_for_the_server` | `packages/tddy-terminal-rpc/tests/pty_relay_room_join.rs` (new) | Join request (room, identity, server identity, credentials, one-hour TTL, thirty-second wait); the lobby is the default room (`pty_relay_with_no_room_named_joins_the_lobby`). The terminal stream itself opens through `open_duplex` and is covered by the testkit suites through the binary |
| `a_remote_that_ends_the_stream_without_an_exit_status_is_reported` | `packages/tddy-remote-git-repo/tests/relay_over_room.rs` | A duplex whose far end goes away yields `StreamEndedWithoutExitStatus`, never an invented exit code |
| `the_tool_client_joins_a_destination_once_and_reuses_the_session` | `packages/tddy-session-tool-client/tests/room_transport_cache.rs` (new) | Two dispatches, one `connect`; a gone peer reconnects |
| `only_plugins_wiring_and_known_offenders_name_a_livekit_crate` + `every_known_offender_still_names_a_livekit_crate` | `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs` | Four entries struck; both ratchet tests pass |
| `a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary` | same | The three lib+bin crates on the allowed set keep LiveKit out of their library sources (Decisions) |
| `a_session_joined_with_credentials_reaches_the_peer_through_rpc`, `a_session_joined_with_a_token_reaches_the_peer`, `a_duplex_carries_frames_both_ways_while_the_stream_is_open`, `a_subscription_receives_what_is_broadcast_on_its_topic`, `a_peer_that_never_joins_is_reported_absent`, `a_server_that_cannot_be_reached_is_a_room_error` | `packages/tddy-livekit/tests/room_connector.rs` (new, written in green) | `LiveKitRoomConnector` against a real LiveKit server: both auth shapes, rpc, duplex, broadcast, the two failures |

Unit (`tddy-room-client/src/lib.rs`): `with_nothing_wired_connecting_fails_naming_the_missing_transport`.

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **A new crate, not `tddy-daemon-extensions`.** Client tools are not daemons, and the in-jail
  `tddy-tools` build should not link a daemon-ports crate for its client transport.
- **Build on `tddy_rpc::RpcClientTransport`.** It already abstracts the calls. Only joining, live
  duplex, broadcast and presence are new.
- **Binary-only LiveKit edge for lib+bin crates: an allowed-set entry, verified by a shape test.**
  The ratchet counts a crate's normal dependencies, and a dependency used only by `[[bin]]` still
  appears there — Cargo has no binary-only dependency. Chosen: `tddy-session-sync`,
  `tddy-remote-git-repo` and `tddy-tools` join the ratchet's allowed set as
  `WIRING_WITH_A_LIBRARY`, and a new test (`a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary`)
  reads every source file of those crates but `src/main.rs` and fails on a path into a LiveKit crate,
  so the allowance stays what it says: the binary's wiring. Rejected: a bin-only crate
  (`tddy-session-sync-cli`) — a new workspace crate for each of three binaries, three more manifests
  and the `./install`/`./release` scripts to learn them, for an edge the shape test already holds.
  `tddy-tools` is on the list too, which the contract did not foresee: it named no LiveKit crate
  before (its feature forwarded to the libraries'), and now wires the plugin itself.
- **The tool client's connector is process-wide.** `dispatch_session_tool`, the conversation calls
  and the roster's `livekit_session` are called from a dozen sites in `tddy-tools` and `tddy-discovery`;
  a parameter on each is the transport-selection rewrite this PR is told not to make. The binary calls
  `install_room_connector` once; uninstalled, a join fails with `NoRoomTransport`'s message. `room_session_via`
  keeps the connector as a parameter, which is what the tests drive.
- **Size risk.** Five libraries and three binaries make a large review. If wave 2's surface shows it
  is too large, split by capability: 7a (session-sync and remote-git-repo, the joiners holding
  sessions) and 7b (pty-relay, tool-client and discovery, the call-and-go clients). Renumbering the
  stack is the cost.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
### /validate-changes
2026-10-19, `origin/master..HEAD` is this PR's six commits only (42 files); no parent-owned file removed.
`cargo check --all-targets` over the touched packages is clean. Boundaries held: nothing of #623's two
functions is changed, no daemon-side or `tddy-coder` code is touched, tool-client's transport selection is
not de-duplicated. Info: `ROOM_TOKEN_TTL` (session-sync, six hours) restates LiveKit's own default because
the library names no LiveKit crate; the test pins it. `run_livekit_session` / `run_livekit_terminal` in
`tddy-terminal-rpc` keep LiveKit-flavoured private names although the module is transport-free — renaming
them touches a file that has a standing record, so it is left.
### /validate-tests
Fluent Given/When/Then holds throughout. Fixed: a helper named after the crate that hid a six-hour literal
became the `SIX_HOURS` constant; `pty_relay_with_no_room_named_joins_the_lobby` gained its Given/When/Then
and asserts the whole list of rooms instead of indexing. The acceptance table now names the second relay
test and states what the pty-relay test actually covers.
### /validate-prod-ready
No `todo!`, `unimplemented!`, `TODO`/`FIXME`, `println!` or `dbg!` in this PR's production code. `unwrap`/`expect`
appear only in the `test-support` in-memory connector (mutex poisoning, an over-scripted join) and at the
one startup call that installs the connector once. The in-memory module is compiled only under the
`test-support` feature, which every dependent enables in `[dev-dependencies]` alone.
### File length (/pr-wrap 3.5) — developer decision needed
| File (production lines) | Before → after | Other satellite nodes' own commits touching it | `Claimed by:` |
|---|---|---|---|
| `packages/tddy-session-sync/src/attach.rs` | 536 → 555 | none (#630–#635) | none |
| `packages/tddy-session-tool-client/src/lib.rs` | 1,111 → 1,114 (the gate's awk reads 610 → 1,114 on master because of a `cfg(any(feature, test))` attribute this PR removed) | none | none |
| `packages/tddy-terminal-rpc/src/pty_relay.rs` | 776 → 784 | none | none |
| `packages/tddy-session-sync/src/sync.rs` | 788 → 788 (not grown; new record `oversized-file-sync.md`) | none | none |

No decomposition was done and no deferral is recorded as consented: the growth is +19, +3 and +8 lines, and
the developer decides between deferring (a `docs/dev/todo/` entry) and decomposing in this PR.
### /analyze-clean-code
Production changes are small and cohesive: `relay::run` shrank from a connect-and-pump function to a join
plus `relay_over_duplex`; the three joins are named functions (`join_session_room`, `join_terminal_room`,
`connect_room_session`) with their constants named. No new function over forty lines, nesting over three
levels or unnamed magic value; the pre-existing `run_livekit_terminal` length is on the file's standing record.

## TODO

- [x] Record initial discovery (`2026-10-16-livekit-satellite-client-transports-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: developer asked to finish the stack without stopping, 2026-10-16)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Run scoped tests for touched packages (full-workspace result from CI)
- [x] Validate changes (/validate-changes)
- [x] Validate tests (/validate-tests)
- [x] Validate production readiness (/validate-prod-ready)
- [x] Analyze code quality (/analyze-clean-code)
- [x] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
