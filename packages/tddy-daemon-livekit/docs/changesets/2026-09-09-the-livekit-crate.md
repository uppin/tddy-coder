# 2026-09-09 — Everything LiveKit in one crate

**Type:** Architecture

New crate, added by node 4 of the `#unbundle` stack
([#473](https://github.com/uppin/tddy-coder/pull/473)). Full story in the cross-package entry:
[2026-09-09-unbundle-auth-livekit.md](../../../../docs/dev/changesets/2026-09-09-unbundle-auth-livekit.md).

Four modules and 6,730 production lines leave `tddy-daemon` — `session_room`,
`livekit_peer_discovery`, `common_room_supervisor`, `livekit_rooms_stream` — plus a fifth,
`livekit_service`, **authored here** because family T needs a service to be served by once
`StreamLiveKitRooms` leaves `ConnectionServiceImpl`.

**`StreamLiveKitRooms` changes coordinate**: `connection.ConnectionService` →
`livekit.LiveKitService`. It is the crate's only protocol change, `connection.proto` drops to 72
rpcs, and `packages/tddy-web`'s rooms panel migrated in the same PR. `livekit.proto` imports
nothing — its closure is 12 messages with zero overlap with anything that stays, a third
self-contained cut after `host.proto` and `worktree.proto`.

**The dependency direction was already right.** `session_room` defines four trait ports —
`SessionTerminalBridge`, `WorktreeSource`, `SessionTokenMinter`, `RemoteSnapshotSource` — and
`ConnectionServiceImpl` *implements* two of them. The god object depended on this subsystem's
abstractions rather than the reverse, which made the move relocation and not redesign.

`SessionTokenMinter` is the load-bearing port. Room JWTs are minted by `tddy-daemon-auth` from
`config.livekit.api_secret`, **the same secret that signs session tokens** — so keeping minting
behind a port is what stops this crate growing a second signer, and
`tests/dependency_boundary_unit.rs` pins `tddy-daemon-auth` off this crate's dependency path.

**Four edges were cut, and each direction was chosen rather than defaulted.** Two were named by
node 1's cycle audit; two were found while making the move.

| Edge | Cut |
|---|---|
| `livekit_peer_discovery` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX` | the constant lifted to `tddy_daemon_kernel::daemon_identity`; `split_session` re-exports it. Defining it here would make the **producer** of the identity depend on the crate that refuses it, and would break again when `split_session` moves at nodes 6–8 |
| `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery` | the three-hop loop a pair-based audit could not see. The `CommonRoomSupervisor` **trait** moved beside its one implementation; `daemon_config_service` re-exports it. Verified rather than assumed — re-pointing the return leg alone does not do it, because the trait import is a `livekit → daemon` edge whether or not it closes a loop |
| `session_room` → `session_attachments::list_session_attachments` | the listing lifted to `tddy_workflow::artifact_paths`, beside the `session_attachments_root` naming the directory it reads. Not the kernel: this is a filesystem convention `tddy-workflow` already owns |
| `livekit_peer_discovery` → `oauth_loopback_tunnel` | not a cycle but a forbidden direction. `spawn_oauth_loopback_tunnel` moved to `tddy-daemon-auth`; `spawn_common_room_discovery_task`, which composes it with the discovery loop, moved to `tddy-daemon`'s `runtime.rs` — the only crate holding both halves after the split |

A fifth, `livekit_peer_discovery ⇄ multi_host`, needed nothing. Node 1 took `multi_host` to
`tddy-host-service`, and nothing there names a LiveKit module, so the edge runs one way.

**Five of eighteen candidate suites moved, not eighteen.** Thirteen are joint session/LiveKit
suites reaching `connection_service`, `test_util`, `split_session`, `claude_cli_session` or
`session_attachment_staging`, all of which stay for nodes 6–8; a suite naming them cannot move
without putting `tddy-daemon` back on this crate's dependency path. `session_room_acceptance.rs`
and `session_room_cross_host_acceptance.rs` stay for exactly that reason.

**Nothing skips without Docker — the suites *fail*.** `LiveKitTestkit::start()` returns `Err` and
every caller `.expect()`s it, so an absent `/var/run/docker.sock` produces a loud failure naming
`LIVEKIT_TESTKIT_WS_URL`. The harness code is byte-identical either side of the move, so the
outcome is unchanged.

⚠ **The four Docker-backed suites now contend.** They are `#[serial]` *within* a binary, but cargo
parallelises binaries, and they are four of eight in a small crate rather than four of twenty-five
in `tddy-daemon`. One whole-crate run lost a test to container contention; `--test-threads=1`
across all four is 11/0. Wants a crate-spanning `serial_test` group.

**Log targets still say `tddy_daemon::common_room` and
`tddy_daemon::livekit_peer_discovery::peer_metadata`** — kept deliberately, so operators' `RUST_LOG`
filters keep working.

**Four files stand over the 500-line budget**, every one relocated rather than written:
`session_room.rs` (2,992), `livekit_peer_discovery.rs` (2,060), `common_room_supervisor.rs` (881),
`livekit_rooms_stream.rs` (779). The authored `livekit_service.rs` is 127. Splitting is deferred:
`move_module_to_crate` operates on whole module files, so a split churns the diff before the move
or costs a second 40-minute rust-analyzer index after it.

99 tests across 8 suites.
