# Room roster — reading LiveKit's server API

`tddy-livekit` can read the LiveKit server's own view of its rooms: which rooms exist and who is
joined to each. This is distinct from everything else the crate does, which speaks to LiveKit as a
*participant*. Here it speaks as an *operator*, over the HTTP server API, with the same API key and
secret the daemon mints join tokens with.

Consumer: the daemon's `StreamLiveKitRooms` RPC, behind its `RoomRoster` trait — see
[connection-service.md § LiveKit rooms](../../tddy-daemon/docs/connection-service.md) and the product
doc [livekit-rooms-panel.md](../../../docs/ft/web/livekit-rooms-panel.md).

## `LiveKitRoomRoster`

```rust
LiveKitRoomRoster::from_ws_url(ws_url, api_key, api_secret)?  // -> Result<_, ServerApiUrlError>
roster.list_rooms().await                                     // -> Result<Vec<LiveKitRoomInfo>, String>
```

`list_rooms` issues `ListRooms`, then `ListParticipants` per room, and maps the results into the
`connection.proto` types the RPC carries. This is the **first production use** of
`livekit_api::services::room::RoomClient` — before it, only `tddy-livekit-testkit` called the room
service, to wait for a container to come up.

### Mapping rules

`participant_info` is a pure function so its rules are testable in isolation:

- **`identity` and `metadata` are relayed verbatim.** Metadata is a JSON document shallow-merged
  across independent publishers (see [participant-metadata.md](participant-metadata.md)); this crate
  does not parse or validate it, and an empty string means nothing was published.
- **Timestamps are seconds × 1000.** LiveKit also exposes newer `*_ms` fields, but only recent
  servers populate them and proto3 cannot distinguish `0` from unset — reading them would need
  "`ms` if non-zero else `seconds × 1000`", which is a fallback. One source that reads correctly
  against any server version costs only sub-second precision, which nothing renders.
- **`joined_at == 0` stays 0** — the sentinel for "no join time recorded" (a `JOINING` participant
  has none yet). No time is invented for it.
- **State is named by the protocol**, via `state().as_str_name()` → `"JOINING"` / `"JOINED"` /
  `"ACTIVE"` / `"DISCONNECTED"`. There is no hand-maintained mapping table to drift, and a state
  LiveKit adds later flows through rather than being dropped.

### Deadline

The whole read — `ListRooms` plus every per-room `ListParticipants` — is bounded by
`ROSTER_READ_TIMEOUT` (5 s), overridable in tests via `with_read_timeout`. A server API that accepts
the read and never answers would otherwise stall its consumer indefinitely with no error, leaving a
UI on a stale roster that looks healthy. Expiry takes the same error path an outright failure takes.

The ceiling is **per read, not per call**, so it does not scale with room count, and it sits far
above a healthy read (single-digit ms on a LAN) so ordinary slowness is not reported as breakage.

## `http_base_from_ws_url`

The server API is HTTP; `livekit.url` in daemon config is a WebSocket address. `server_api_url.rs`
converts one to the other: `ws://` → `http://`, `wss://` → `https://`, with host, port and path
preserved verbatim.

Two cases are the reason this exists as a real function rather than a `replace`:

- **A portless authority stays portless.** `wss://livekit.example.com` is valid — the scheme's
  default port applies, and `wss` → `https` maps 443 to 443. The only converter in the tree before
  this one was test-only and required an explicit port.
- **A reverse-proxy path prefix survives**, so LiveKit served under `wss://edge.example.com/livekit`
  resolves correctly.

Anything that is not a WebSocket scheme is refused as `NotWebSocketScheme` rather than passed
through — including an operator who configured the HTTP address by mistake, which would otherwise
appear to work until it silently didn't. A missing scheme or an empty authority is `Malformed`.

Callers must use **`livekit.url`**, the daemon's own address for the server, not `public_url`, which
is the browser-facing one.
