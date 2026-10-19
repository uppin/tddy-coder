# tddy-room-client

The client side of "join a room and talk in it", as ports. Client tools — the worktree mirror
(`tddy-session-sync`), the git SSH shim (`tddy-remote-git-repo`), the terminal relay
(`tddy-terminal-rpc`) and the in-session tool client (`tddy-session-tool-client`, `tddy-discovery`) —
reach a room through this crate. Their libraries name no LiveKit crate; their binaries hand them an
implementation. The crate depends on `tddy-rpc`, `tokio` and `async-trait` only.

## The port

| Item | Holds |
|---|---|
| `RoomJoin` | `url`, `room`, own `identity`, and `auth` |
| `RoomAuth` | `Credentials { api_key, api_secret, ttl }` — the connector mints the token — or `Token(String)`, minted elsewhere (the identity is then the token's own, and `RoomJoin.identity` is empty) |
| `RoomConnector::connect(join, peer_identity, wait)` | Joins and waits up to `wait` for `peer_identity`, returning an `Arc<dyn RoomSession>` |
| `RoomSession` | `rpc()` (an `Arc<dyn tddy_rpc::RpcClientTransport>`), `open_duplex(service, method)`, `subscribe(topic)`, `peer_present()` |
| `RoomDuplex` | A live bidi stream: `outbound` (frames to send) and `inbound` (replies as they arrive). Dropping `outbound` half-closes the sending side; replies keep arriving |
| `RoomConnectError` | `Room(reason)` — the room could not be joined — or `PeerAbsent { identity, waited }`; implements `Display` |
| `NoRoomTransport` | The null object: every join fails `Room("no room transport is wired into this build")` |

A session owns its connection: dropping the last handle leaves the room, so callers hold the
`Arc<dyn RoomSession>` for as long as they listen.

## Implementations

- **LiveKit** — `tddy_livekit::room_connector::LiveKitRoomConnector`. It mints the token when given
  credentials, joins through `client_connect::connect_client`, and wraps the room as a session: calls
  through `RpcClient`, duplexes through its bidi streams (a pump task with 64-frame buffers), broadcasts
  through `BroadcastChannel`, presence through the room's remote participants. `tests/room_connector.rs`
  runs it against a real LiveKit server: both auth shapes, an RPC, a duplex in both directions, a
  broadcast subscription, an absent peer and an unreachable server.
- **In memory** (`in_memory`, behind the `test-support` feature, enabled in dev-dependencies only) —
  `InMemoryRoomConnector` records every join and answers from a script (`Joined`, `PeerAbsent`,
  `Unreachable`); `a_session_with_one_duplex()` serves one duplex whose far end a test plays.

## Wiring

| Binary | Wires |
|---|---|
| `tddy-session-sync`, `tddy-remote-git-repo` | `LiveKitRoomConnector` into `attach` / `relay::run` |
| `tddy-tools` | `LiveKitRoomConnector` behind its default-on `livekit` feature (otherwise `NoRoomTransport`), into `pty-relay` and, once, into the tool client with `install_room_connector` |

The satellite ratchet in [`tddy-daemon-extensions`](../../tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet)
holds the libraries to it.
