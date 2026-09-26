# LiveKitService (tddy-daemon-livekit)

Everything the daemon does with LiveKit: the per-worktree session room, common-room peer discovery,
the supervisor that keeps the common room joined, and the `livekit.LiveKitService` observability
stream.

## Where the code lives

| Module | What is in it |
|---|---|
| `session_room` | the per-session room — naming, the poll loop, the WIP ref and the delta ring, and the four trait ports. Detail: [session-room.md](./session-room.md) |
| `livekit_peer_discovery` | the metadata advertisement (with `AdvertisedSigningKey`), `CommonRoomPeerRegistry`, `peer_signing_public_keys`, `daemon_rpc_identity`, and the forwarding helpers |
| `common_room_supervisor` | the `CommonRoomSupervisor` trait and `SupervisedCommonRoom` — joining the common room and keeping it joined |
| `livekit_rooms_stream` | `RoomRoster`, `RosterError`, and `pump_rooms`, the 3 s poller behind the rooms stream |
| `livekit_service` | `LiveKitServiceImpl` and `build_livekit_entry` — **authored here**, because family T needs a service to be served by once it leaves `ConnectionServiceImpl` |
| `peer_routing` | `PeerRouting`: this daemon's routing identity, the eligible peers (`set_eligible_daemon_source`, `eligible_instance_ids`), the route an addressed request takes (`classify_daemon_route`, `classify_addressed_daemon_route`, `stream_served_by_peer`) and the common-room slot a forward travels through. Shared, not copied, by the session host and the RPC families above it, so a session RPC and an exec-tool RPC addressed at one daemon agree on who owns the call |
| `session_admission_service` | the room-admission handshake: `SessionAdmissionRegistry`, the short-TTL admission token (`ADMISSION_TOKEN_TTL`, `ADMISSION_RENEW_MARGIN`) and `SessionAdmissionServiceImpl`, whose `AdmitOwningDaemon` re-mints a token only for a daemon the registry still holds, so a revoked daemon stays out of the room |

`peer_routing` and `session_admission_service` hold no session-host state. `tddy-session-lifecycle`
re-exports both by name (`pub use tddy_daemon_livekit::{peer_routing, session_admission_service};`),
and its host keeps one-line delegations to `PeerRouting` for the methods it serves itself, which is
why those `PeerRouting` methods are `pub`.

## `livekit.LiveKitService`

One method, and it is the only protocol change the crate carries:

- `StreamLiveKitRooms(session_token)` → `stream LiveKitRoomsEvent`

`livekit.proto` imports nothing. Its message closure is **12 messages with zero overlap** with
anything that stays in `connection.proto` — the third self-contained proto cut after `host.proto`
and `worktree.proto`, and the reason there is still no shared types file.

`build_livekit_entry(Arc<dyn RoomRoster>, SessionUserResolver)` is how the wiring layer registers
it. The roster argument is the LiveKit **server's** room list, which is a different question from
`CommonRoomPeerRegistry`'s peer *eligibility*; the resolver is how the stream authenticates.

### Snapshot then changes

The first message always carries `LiveKitRoomsSnapshot` (every room, with its participants); every
message after it carries exactly one `LiveKitRoomsChange` — `room_added` (the full row, so a
consumer never infers a room from a partial event), `room_removed`, `participant_joined`,
`participant_left`, `participant_metadata_changed`, `participant_state_changed`. Metadata and state
are diffed independently, so a participant republishing both on one tick produces two events; the
feed's contract is one event per delta throughout. A client folds changes onto the snapshot and
never re-requests the list.

### The daemon owns the cadence

LiveKit's server API has no change feed, so `pump_rooms` polls it every 3 s and calls `diff_rosters`
against the roster it last sent **on that stream**. Per-subscriber baselines are what stop two
watchers desynchronizing each other — a shared baseline would let watcher B's tick consume a delta
watcher A had not been sent. A tick with no delta emits nothing, so an idle server yields an idle
stream. Ticks are `MissedTickBehavior::Skip`: a read slower than the cadence must not queue up the
ticks it outlasted, since bursting them fires another `1 + rooms` calls exactly when the server API
is already slow.

### The loop lives and dies with its subscriber

`pump_rooms` selects on `tx.closed()` alongside the tick. That is load-bearing rather than
defensive: an idle stream sends nothing, so a loop that learned about departure only from a failed
send would never learn at all, and every subscription would leave a permanent 3 s poll of the
LiveKit server behind it. It works only because the generated server-streaming pump propagates the
teardown — see [tddy-codegen § Server-streaming teardown](../../tddy-codegen/docs/server-streaming.md).

### Errors are never an empty list

A roster read that fails — or a daemon with no LiveKit credentials — terminates the stream with the
reason. Reporting zero rooms would read to the panel as "the server has no rooms", which is a
different fact. `RosterError::Unconfigured` is `FAILED_PRECONDITION`, a precondition the daemon's own
operator has not met and retrying changes nothing; `RosterError::ReadFailed` is `INTERNAL`, the
server's fault and possibly transient. A caller tells them apart from the code, not by parsing prose.

The production `RoomRoster` is [`tddy_livekit::room_roster`](../../tddy-livekit/docs/room-roster.md),
built from `DaemonConfig.livekit`.

**Cost.** Each subscription runs its own poller, so load is `1 + room count` calls every 3 s **per
open subscription**, not per daemon. It is bounded by panels actually open. One shared poller
broadcasting full rosters, with each subscriber diffing locally, would preserve the same
per-subscriber guarantee at one poller per daemon — the escape hatch if this panel becomes
commonly-open.

## The direction was already right

`session_room` defines four trait ports — `SessionTerminalBridge`, `WorktreeSource`,
`SessionTokenMinter`, `RemoteSnapshotSource` — and `ConnectionServiceImpl` *implements* two of them.
The god object depended on this subsystem's abstractions rather than the reverse, which is the
direction extraction wants and the reason this move is relocation rather than redesign.

`SessionTokenMinter` is the load-bearing one. Room JWTs are minted by
[`tddy-daemon-auth`](../../tddy-daemon-auth/docs/auth-service.md) from `config.livekit.api_secret`,
which signs room JWTs and nothing else. Keeping minting behind a port is what stops this crate
growing a signer of its own, and `tests/dependency_boundary_unit.rs` pins `tddy-daemon-auth` off
this crate's dependency path so the port cannot quietly stop being one.

## Signing keys ride the advertisement, as opaque strings

Each daemon signs session tokens with an Ed25519 key of its own, and a peer verifies them against
the public key the daemon advertises here. This crate carries that key **without understanding
it**: `AdvertisedSigningKey { key_id, public_key }` is two strings — the key id and the base64url
SPKI DER — built by `tddy-daemon` from `tddy_daemon_auth::DaemonSigningKey` and handed to the
discovery loop, which publishes them as `signing_key_id` / `signing_public_key` on every
(re)connection. Both fields are `#[serde(default)]`, so an advertisement without a key still parses
and its peer's tokens are simply refused. `AdvertisedSigningKey::default()` advertises no key and
exists for suites that run discovery without a signing identity; `runtime::build` starts the loop
only for a daemon that signs.

On the read side, `peer_signing_public_keys(peers, key_id)` and
`CommonRoomPeerRegistry::signing_public_keys_for(key_id)` return **every** candidate advertised
under an id, still as strings. The decoding and the check that a candidate hashes to its id belong
to `tddy-daemon`'s `CommonRoomKeyDirectory`, the `tddy_daemon_auth::KeyDirectory` adapter — it
cannot live here, since `KeyDirectory` is an auth-owned trait and this crate may not reach auth.

**Who may be taken for a daemon** is `tddy_service::may_be_daemon_discovery_identity`: discovery
reads an advertisement only from an identity outside `NON_DAEMON_IDENTITY_PREFIXES` — browser
(`web-`, `browser-`), coder/session (`server…`, `daemon-…`), `split-agent-`, `remote-git-` and
`screenshare-host-`. Every client-facing mint (`token.TokenService`) refuses exactly the identities
that predicate allows, so a key is only ever read from an identity a daemon minted for itself. The
rule lives in `tddy-service`, the lowest crate both the mint and this crate reach, so the two sides
cannot drift.

## Four edges had to be cut

Rust crates cannot be mutually dependent, and no crate here may reach `tddy-daemon` at all. Each cut
below spans a crate boundary only because this crate moves one end of it, and each direction was
chosen rather than defaulted.

| Edge | Cut, and why that direction |
|---|---|
| `livekit_peer_discovery` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX` | the constant lives in `tddy_service::participant_identity`, beside the rule that refuses it, and `tddy_daemon_kernel::daemon_identity` re-exports it for the crates that mint agents' identities; `split_session` re-exports that. Defining it here instead would make the **producer** of the identity depend on the crate that *refuses* it, and would break again when `split_session` moves to a third crate |
| `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery` | the `CommonRoomSupervisor` **trait** moves to `common_room_supervisor`, beside its one implementation, and `daemon_config_service` re-exports it. Re-pointing the return leg alone does not do it: the trait import is a `livekit → daemon` edge whether or not it closes a loop. The return leg was re-pointed too, from a re-export in a departing module to its real home, `tddy_daemon_kernel::daemon_identity::local_instance_id_for_config` |
| `session_room` → `session_attachments::list_session_attachments` | the listing is lifted to `tddy_workflow::artifact_paths`, beside the `session_attachments_root` that names the very directory it reads, and the daemon re-exports it. Not the kernel: this is a filesystem convention `tddy-workflow` already owns, not a daemon identity |
| `livekit_peer_discovery` → `oauth_loopback_tunnel` | not a cycle, but a forbidden direction — the LiveKit crate must not reach the identity boundary. `spawn_oauth_loopback_tunnel` moves to `tddy_daemon_auth::oauth_loopback_tunnel`, the module whose supervisor it starts. `spawn_common_room_discovery_task`, which composes that supervisor with the discovery loop, moves to `tddy-daemon`'s `runtime.rs` — after the split that is the only crate holding both halves |

A fifth, `livekit_peer_discovery ⇄ multi_host`, needed nothing. `multi_host` went to
`tddy-host-service` in node 1, and nothing in that crate names a LiveKit module, so the edge runs
one way: `tddy-daemon-livekit → tddy-host-service`.

## Known gaps

- **No LiveKit call has a client-side deadline.** Every one of them is now in this crate, so a
  single policy is finally expressible in one place — but it is a policy decision affecting
  long-lived streams as well as unary calls, so it is
  [its own change](../../../docs/dev/todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md),
  as is [the room-creation call's missing timeout](../../../docs/dev/todo/2026-09-06-the-livekit-room-creation-call-has-no-timeout.md).
- **Peer discovery does not follow a runtime common-room reconnect.** A daemon that *gains* a
  joinable room at runtime serves its roster there but does not discover the others in it until
  restarted. `TODO` at `DaemonCommonRoomConnector::connect` in `common_room_supervisor.rs`. The
  connector and the registry are now in one crate, so the reconnect path is expressible without
  reaching through the wiring layer's `config`, which is what previously made it a cycle.
- **The Docker-backed suites leave the real LiveKit join path unexercised** wherever Docker is
  absent — and they *fail* there rather than skipping.
- **`tddy-service` depends on `tddy-tui`**, so this crate inherits the TUI in its build.

## Files over budget

`session_room.rs` (2,992), `livekit_peer_discovery.rs` (2,060), `common_room_supervisor.rs` (881)
and `livekit_rooms_stream.rs` (779) all stand over the 500-line budget, and every one crossed as a
whole module rather than being written here. The one authored file, `livekit_service.rs`, is 127.
Splitting them belongs to its own branch: `move_module_to_crate` operates on whole
`<crate>/src/<module>.rs` files, so a split has to happen either before the move — churning the diff
a reviewer reads as "did any logic change?" — or after it, and each restructure plan costs a
40-minute rust-analyzer index on this workspace.

## Log targets still name `tddy_daemon`

`tddy_daemon::common_room` and `tddy_daemon::livekit_peer_discovery::peer_metadata` are kept
deliberately: a log target is an operator's `RUST_LOG` filter, and renaming it to match the crate
would silently break every filter already selecting it.

## Tests

```bash
cargo test -p tddy-daemon-livekit -- --test-threads=1
```

`--test-threads=1` is not decoration. The four Docker-backed suites are `#[serial]` *within* a
binary, but cargo parallelises binaries, and they are four of a small crate's suites rather than
four of `tddy-daemon`'s twenty-five — so a whole-crate run starts them closer together against one
shared LiveKit container. The suites themselves are listed in
[session-room.md § Tests](./session-room.md#tests).

## Related

- [session-room.md](./session-room.md) — the per-session room in detail
- [`tddy-daemon-auth`](../../tddy-daemon-auth/docs/auth-service.md) — who mints the room JWTs
- [`connection-service.md`](../../tddy-daemon/docs/connection-service.md) — the other 72 methods
- [LiveKit rooms panel (product)](../../../docs/ft/web/livekit-rooms-panel.md) — what consumes `StreamLiveKitRooms`
- [LiveKit peer discovery (product)](../../../docs/ft/daemon/livekit-peer-discovery.md)
- [Session rooms (product)](../../../docs/ft/daemon/session-room.md)
- [changesets/](./changesets/)
