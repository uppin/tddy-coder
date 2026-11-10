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

## What would close it

1. **Ports and cheap adapters.**
   - RPC: promote the in-memory scripted peers already in
     `tddy-daemon-kernel/tests/common_room_forwarding.rs` and
     `tddy-session-split/tests/split_room_hosting_port.rs` into a shared testkit crate. It provides an
     in-memory `PeerForwarder`, the session-room transport, and a stdio/IPC adapter for cross-process
     cases.
   - Media: a media port (capture source, publisher and viewer), with an in-memory adapter that carries
     frames as values.
2. **Move behaviour suites onto the cheap adapters.** The 11 `tddy-daemon` cross-host suites and the 2
   `tddy-session-lifecycle` room suites run their scenarios over the in-memory transport. `test_placement.rs`
   keeps its rule (a suite that mounts the daemon's composition stays in `tddy-daemon`). Only the transport
   underneath it changes.
3. **One LiveKit compatibility package** (for example `tddy-livekit-compat-tests`).
   - It holds the LiveKit-only suites from `tddy-livekit`, `tddy-daemon-livekit` and `tddy-e2e`.
   - It holds one **contract suite per port**, run against every adapter: the same tests against
     in-memory, IPC and LiveKit, so "LiveKit behaves like the others" is a single assertion set, not a
     second copy.
   - It is the only package with a `tddy-livekit-testkit` dev-dependency. A placement test pins that.
4. **CI.** The LiveKit e2e leg runs just that package, which also delivers most of
   [split the e2e compile by test target](./2026-10-04-split-e2e-leg-compile-by-test-target.md).
   `.config/rust-e2e.filterset` shrinks to the one package.
5. **Media.** `tddy-screenshare`, `tddy-livekit-screen-capture`, `tddy-screen-sharing` and `tddy-vnc` depend on the
   media port. LiveKit media becomes one adapter crate, and the streamer duplication goes with it.

## Why deferred

This is a cross-cutting test-architecture and architecture change across about 15 crates. It is orthogonal to
`#agent-tools`, which only surfaced its cost. It belongs in a stack of its own: ports and cheap adapters
first, then the suites move bottom-up, then the compatibility package, then media.
