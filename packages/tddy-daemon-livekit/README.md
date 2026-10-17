# tddy-daemon-livekit

Everything the daemon does with LiveKit: the per-worktree session room, common-room peer discovery,
the supervisor that keeps the common room joined, and the `livekit.LiveKitService` rooms stream.

## Quick Start

### Build
```bash
cargo build -p tddy-daemon-livekit
```

### Test
```bash
cargo test -p tddy-daemon-livekit -- --test-threads=1
```

`--test-threads=1` is not decoration. Four suites drive a real LiveKit server through the Docker
testkit; they are `#[serial]` within a binary, but cargo parallelises binaries, so a whole-crate run
starts them against one shared container. And **nothing skips without Docker** — an absent
`/var/run/docker.sock` makes those four *fail*, naming `LIVEKIT_TESTKIT_WS_URL` as the alternative.

## What it serves

| Service | Methods |
|---|---|
| `livekit.LiveKitService` | `StreamLiveKitRooms` |
| `session_room.SessionRoomService` | `OpenSessionRoom` |

`StreamLiveKitRooms` is the method that left `connection.ConnectionService`, which
is down to 72; `OpenSessionRoom` is the plugin's own call, registered only on a daemon that wires it. `packages/tddy-web`'s rooms panel addresses the new coordinate.

## Two rules this crate exists to hold

**It never depends on `tddy-daemon`, and never on `tddy-daemon-auth`.**
`tests/dependency_boundary_unit.rs` reads the workspace graph (`tddy-testing-commons`'s `WorkspaceGraph`) and fails if it ever does.
The second half matters as much as the first: session tokens are signed by the auth crate with the
daemon's own key, and the only way this crate reaches minting is the `SessionTokenMinter` **port**.
If that port stopped being one, this crate could grow a second signer. The same rule shapes key
distribution: this daemon's public signing key rides its common-room advertisement as two opaque
strings (`AdvertisedSigningKey`), and `peer_signing_public_keys` / `CommonRoomPeerRegistry::
signing_public_keys_for` hand back every candidate advertised under an id, undecoded — `tddy-daemon`
turns them into the auth crate's `KeyDirectory`.

**The ports point the right way already.** `session_room` defines `SessionTerminalBridge`,
`WorktreeSource`, `SessionTokenMinter` and `RemoteSnapshotSource`, and `ConnectionServiceImpl`
*implements* two of them — so the daemon depends on this subsystem's abstractions, not the reverse.
That is why the extraction was relocation rather than redesign.

## Documentation

### Technical implementation (how)
- [LiveKitService](./docs/livekit-service.md) — the rooms stream, the cut edges, and the known gaps
- [Session room module](./docs/session-room.md) — naming, the poll loop, the WIP ref and the delta ring
- [Session reach](./docs/livekit-service.md#livekitsessionreach--opening-the-room-for-a-reached-session) — `LiveKitSessionReach`, `OpenSessionRoom` and the status it reports
- [Changesets](./docs/changesets/) — applied changeset history

### Product requirements (what)
- [Session rooms](../../docs/ft/daemon/session-room.md)
- [LiveKit peer discovery](../../docs/ft/daemon/livekit-peer-discovery.md)
- [LiveKit rooms panel](../../docs/ft/web/livekit-rooms-panel.md) — what consumes `StreamLiveKitRooms`

### Neighbours
- [`tddy-daemon-auth`](../tddy-daemon-auth/README.md) — who mints the room JWTs
- [`tddy-daemon-extensions`](../tddy-daemon-extensions/README.md) — the ports this crate implements and the status board it reports on
- [`tddy-livekit`](../tddy-livekit/docs/room-roster.md) — the LiveKit server-API client behind `RoomRoster`
- [`connection-service.md`](../tddy-daemon/docs/connection-service.md) — the other 72 methods
