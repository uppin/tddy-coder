# 2026-11-05 — Tests talk over cheap RPC; LiveKit is tested only for compatibility, and media sits behind a port

**Category:** Future enhancement (test architecture, architecture)
**Source:** developer request during `#agent-tools` n13 (#819), when the n13 [G] stage's scoped runs of
`tddy-daemon` and `tddy-session-lifecycle` paid for a LiveKit container per suite and the LiveKit
peer-discovery and connect-timeout tests flaked under load.
**Extends:** [peer-forwarding tests use LiveKit below the binary boundary](./2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md),
which covers the daemon-to-daemon `PeerForwarder` slice of this entry. Close both together, or close that one
first as the first slice of this one.

## The rule wanted

1. **Behaviour tests talk over a cheap transport** — in-memory, stdio or local IPC (Unix socket / named
   pipe). They need no container, no network and no `tddy-livekit-testkit`, so `./test -p <pkg>` on a
   laptop and a CI shard run them in one process, in parallel, without a shared server.
2. **LiveKit tests exist only to check compatibility.** Their job is to prove that the LiveKit adapter of
   each port behaves like the cheap adapters: the same RPC envelopes, metadata, streaming, rejoin and
   deadline semantics. They live in a **dedicated package**. No crate holding product behaviour has a
   `tddy-livekit-testkit` dev-dependency, and a crate's own suites never pick LiveKit as their transport.
3. **Video and audio sharing go behind an architectural port** in the same way: a transport-neutral
   media port (capture source → track publisher → viewer), with LiveKit as one adapter. Screen-sharing
   behaviour is tested over an in-memory adapter, and LiveKit media is tested only in the compatibility
   package.

## State on master (`cc040c6e9`), measured

**Test files reaching for the LiveKit testkit, by package (43 files):**

| Package | Files | Examples |
|---|---:|---|
| `tddy-daemon` | 11 | `multi_host_acceptance`, `session_room_cross_host_acceptance`, `session_attach_cross_host_acceptance`, `split_session_resume_acceptance`, `staging_forwarding_acceptance`, `remote_managed_worktree_cross_host_acceptance`, `session_agent_remote_acceptance`, `relay_e2e_acceptance`, `first_login_enrolment_acceptance`, `runtime_signing_identity_acceptance`, `common_room_key_trust_acceptance` |
| `tddy-daemon-livekit` | 9 | `livekit_peer_daemons_acceptance`, `forward_to_peer_shared_registry`, `forwarded_rpc_is_stamped_by_the_receiver`, `session_room_livekit_acceptance`, four `common_room_*` repros, `livekit_integration_status` |
| `tddy-livekit` | 8 | `rpc_scenarios`, `rpc_client_factory`, `rpc_three_participant_forward`, `room_connector`, `room_roster_livekit`, `broadcast_and_room_metadata`, `participant_metadata_acceptance`, `acp_session_livekit` |
| `tddy-e2e` | 4 | `livekit_terminal_rpc`, `terminal_service_livekit`, `token_generation_livekit`, `token_service_livekit` |
| `tddy-livekit-testkit` | 3 | the testkit's own tests |
| `tddy-session-lifecycle` | 2 | `session_room_acceptance`, `session_sync_livekit_acceptance` |
| `tddy-coder-cli` | 2 | `coder_publishes_session_metadata_to_participant`, `coder_serves_connection_service_from_participant` |
| `tddy-worktree-service`, `tddy-tools-cli`, `tddy-semantic-index`, `tddy-daemon-rpc` | 1 each | e.g. `remote_git_livekit_acceptance` |

`.config/rust-e2e.filterset` names 99 binaries, which is how CI keeps these off the unit leg today.

**Media is LiveKit-shaped, not behind a port:**

- `tddy-screenshare` calls itself "protocol-agnostic" (a `ScreenSharingClient` trait plus a generic bridge
  loop), but it depends on `livekit`, `livekit-api` and `tddy-livekit`, and its streamer is LiveKit's.
- `tddy-livekit-screen-capture` (CLI: capture, then stream to LiveKit) duplicates that streamer. See
  [livekit-connect streamer duplication](./2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md).
- `tddy-screen-sharing` (`ScreenSharingService`, extracted from `tddy-daemon`) and `tddy-vnc`
  ("VNC-to-LiveKit bridge") both depend on `tddy-livekit` directly.
- No test exercises screen-sharing behaviour without a LiveKit server.

## Progress

**Done 2026-11-15** (`livekit-compat-tests` changeset, `docs/dev/changesets/2026-11-15-livekit-compat-tests.md`):
clause 3's package and clause 4's CI half. `tddy-livekit-compat-tests` now holds the 24 LiveKit
contract suites from `tddy-livekit`, `tddy-daemon-livekit`, `tddy-e2e` and `tddy-coder-cli`, moved
with `move_test_binary_to_crate`; `.config/rust-livekit-e2e.filterset` selects that package plus the
testkit's own suites, and `Rust LiveKit e2e tests` is its own PR check. `livekit_test_placement.rs`
in that package pins the split and names the logic suites below as the remaining set.

**Done 2026-11-15, second node** (`docs/dev/changesets/2026-11-15-in-memory-common-room.md`): the
discovery surface of clause 1. `CommonRoomPeerRegistry::sync_from_participant_fields` is the
transport-neutral snapshot entry (the LiveKit loop's classifier, shared), and `tddy-room-testkit`
is the in-memory hub — rooms, advertisements, registry attach, participant RPC serving, a
`PeerForwarder`, broadcasts, and a silenced-participant model. Four `tddy-daemon` suites converted
(`relay_e2e_acceptance`, `session_attach_cross_host_acceptance`, `multi_host_acceptance`,
`staging_forwarding_acceptance`; 20 tests, seconds instead of container minutes) and left the
placement test's remaining set, now 11. `remote_git_livekit_acceptance` stays: the shipped
`tddy-remote-git-repo` shim itself speaks LiveKit, so that suite tests a LiveKit product feature,
not logic over a transport — it needs a seam in the shim binary, a separate decision.

**Done 2026-11-15, third node** (`docs/dev/changesets/2026-11-15-hub-session-rooms.md`): the
session-room hosting surface of clause 1, first slice. `tddy-room-testkit` gained
`HubSessionReach`/`HubSessionRooms` over per-session hub rooms (holding each hosted session's
participant for the hosted lifetime — dropping it empties the room), and
`tddy-daemon-rpc/session_room_exec_tool_acceptance` converted onto them; that crate's
`tddy-livekit-testkit` dev-dependency is gone. Remaining set: 10.

## What would close it

1. **Ports and cheap adapters.** Measured 2026-11-15 against the 15 remaining suites: every one
   touches at least one of three production surfaces that have **no port today**, so the cheap
   adapters cannot be built test-side only — each surface needs a behaviour-preserving port
   extraction first, in this order:
   - **Common-room discovery port** (9 of the 15 suites use it). `spawn_common_room_discovery_loop`
     (`tddy-daemon-livekit::livekit_peer_discovery`) takes a `DaemonConfig`, connects a concrete
     `livekit::Room` itself, and fills `Arc<RwLock<Option<Arc<Room>>>>`. Extract a port: join the
     common room, publish own metadata, watch participants/metadata, serve the daemon RPC
     participant. LiveKit keeps the implementation it has; an in-memory hub (rooms, identities,
     metadata watch, join/leave) implements it for tests. Two suites join a raw `Room` as a rogue
     or duplicate participant (`common_room_key_trust_acceptance`,
     `runtime_signing_identity_acceptance` — the latter builds the whole `runtime::build`), so the
     hub must also offer a test-side raw participant, and `runtime::build` needs the port carried
     on `RuntimeOptions` beside `spawn_backend`. That runtime seam is the keystone: it changes how
     the binary and the desktop embedder wire discovery, so it is a changeset of its own, first.
   - **Session-room hosting port** (7 of the 15 suites use it). First slice done 2026-11-15
     (third node, above): the existing `SessionReach`/`SessionRooms` ports gained hub
     implementations. Still open for the daemon suites wiring the full
     `with_the_livekit_room_ports` set: room creation, token minting and participant serving
     behind the hosting port those suites consume, with the hub growing those surfaces.
   - **Participant serving/client pair** (6 of the 15 suites hand-wire one; the surfaces overlap, so the three extractions together unlock all 15). Suites hand-wire two services through
     `LiveKitParticipant::connect` + `RpcClient`. `tddy-rpc`'s `ServerEngine`/`ClientEngine` over
     an in-memory duplex is the cheap pair; `tddy-room-client`'s `in_memory` module and
     `tddy-daemon-rpc/tests/support/mod.rs`'s `InProcessPeers` are the seeds. Promote
     `InProcessPeers` into a shared testkit crate (it needs `tddy-daemon-rpc`'s `TestDaemon`
     shape, so the crate boundary is the design question), providing the in-memory
     `PeerForwarder`, the participant pair, and a stdio/IPC adapter for cross-process cases.
   - Media: a media port (capture source, publisher and viewer), with an in-memory adapter that carries
     frames as values.
2. **Move behaviour suites onto the cheap adapters.** Four `tddy-daemon` suites and
   `tddy-daemon-rpc`'s exec-tool suite are done (above).
   The remaining 10 `livekit_test_placement.rs` names — 7 in `tddy-daemon`, the 2
   `tddy-session-lifecycle` room suites and
   `tddy-worktree-service`'s `remote_git_livekit_acceptance` — run their scenarios over the in-memory
   transport. `test_placement.rs` keeps its rule
   (a suite that mounts the daemon's composition stays in `tddy-daemon`). Only the transport
   underneath it changes. Each conversion deletes its line from the placement test and, when a crate's
   last one goes, its `tddy-livekit-testkit` dev-dependency.
3. **One contract suite per port**, run against every adapter: the same tests against in-memory, IPC
   and LiveKit, so "LiveKit behaves like the others" is a single assertion set, not a second copy.
   (The package and its placement pin exist; the per-port shared contract suites do not yet.)
4. **Media.** `tddy-screenshare`, `tddy-livekit-screen-capture`, `tddy-screen-sharing` and `tddy-vnc` depend on the
   media port. LiveKit media becomes one adapter crate, and the streamer duplication goes with it.

## Why deferred

This is a cross-cutting test-architecture and architecture change across about 15 crates. It is orthogonal to
`#agent-tools`, which only surfaced its cost. It belongs in a stack of its own: ports and cheap adapters
first, then the suites move bottom-up, then media. The compatibility package and its CI leg were
pulled forward on 2026-11-15 because they are mechanical and make the remaining conversion
attributable: a red `Rust LiveKit e2e tests` is now an adapter failure by construction.
