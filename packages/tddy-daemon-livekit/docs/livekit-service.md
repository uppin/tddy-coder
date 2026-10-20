# LiveKitService (tddy-daemon-livekit)

Everything the daemon does with LiveKit: the per-worktree session room, common-room peer discovery,
the supervisor that keeps the common room joined, and the `livekit.LiveKitService` observability
stream.

## Where the code lives

| Module | What is in it |
|---|---|
| `session_room` | the per-session room — naming, the poll loop, the WIP ref and the delta ring, and the four trait ports. Detail: [session-room.md](./session-room.md) |
| `livekit_peer_discovery` | the metadata advertisement (with `AdvertisedSigningKey`), `CommonRoomPeerRegistry`, `peer_signing_public_keys`, `daemon_rpc_identity`, and `pub use` re-exports of the kernel's typed forwarding helpers under their old `forward_*_via_livekit` names, for the session crates that still call them |
| `livekit_peer_forwarder` | `LiveKitPeerForwarder`: the kernel's `PeerForwarder` port over the common room — see below |
| `common_room_supervisor` | the `CommonRoomSupervisor` trait and `SupervisedCommonRoom` — joining the common room and keeping it joined |
| `livekit_rooms_stream` | `RoomRoster`, `RosterError`, and `pump_rooms`, the 3 s poller behind the rooms stream |
| `livekit_service` | `LiveKitServiceImpl` and `build_livekit_entry` — **authored here**, because family T needs a service to be served by once it leaves `ConnectionServiceImpl` |
| `session_reach` | LiveKit's implementation of the core's `SessionReach` port: `LiveKitSessionReach`, its `session_room.SessionRoomService` entry (`OpenSessionRoom`), `livekit_integration_state` and `LIVEKIT_INTEGRATION_ID`. Detail: [below](#livekitsessionreach--opening-the-room-for-a-reached-session) |
| `peer_routing` | a re-export of [`tddy_host_service::peer_routing`](../../tddy-host-service/docs/host-service.md#peerrouting--which-daemon-serves-an-addressed-request), kept for the session crates that still name this path |
| `oauth_loopback_tunnel` | the operator-side Codex OAuth callback over the common room — [oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md) |
| `session_admission_service` | the room-admission handshake: `SessionAdmissionRegistry`, the short-TTL admission token (`ADMISSION_TOKEN_TTL`, `ADMISSION_RENEW_MARGIN`) and `SessionAdmissionServiceImpl`, whose `AdmitOwningDaemon` re-mints a token only for a daemon the registry still holds, so a revoked daemon stays out of the room |
| `livekit_peer_transport` | `LiveKitPeerTransport`: `tddy-credential-sync`'s `PeerTransport` over the common room, for `#keyring` 6/9's credential propagation — see below |

`session_admission_service` holds no session-host state. `tddy-session-lifecycle` re-exports it by
name, and re-exports `peer_routing` from `tddy-host-service` and `oauth_loopback_tunnel` from here.

The typed forwarding helpers and `PeerRouting` are transport-neutral, so they live outside this crate:
[`tddy_daemon_kernel::peer_calls`](../../tddy-daemon-kernel/docs/daemon-kernel.md#peer_calls--typed-rpcs-forwarded-to-a-peer)
and `tddy_host_service::peer_routing`. This crate's old paths are plain `pub use` re-exports (not
`#[deprecated]`, which the workspace `clippy -D warnings` gate would turn into errors in the crates
still calling them).

## `LiveKitPeerForwarder` — carrying a forward to a peer daemon

`LiveKitPeerForwarder::new(slot)` implements `tddy_daemon_extensions::PeerForwarder` over the common
room's `Arc<RwLock<Option<Arc<Room>>>>` slot. The kernel's `CommonRoom` applies the deadline, the stream
idle rule and the errors a caller sees; this carries the call and nothing else.

- **A client per call.** Each call builds a `tddy_livekit::LiveKitRpcClientFactory::for_room(room)` client
  addressed at `daemon_rpc_identity(peer)` (`daemon-{peer_id}`). With the room down, a call is a
  `FailedPrecondition`: "LiveKit common room is not connected; cannot forward an RPC / a stream to a peer".
  `is_connected()` is whether the slot holds a room.
- **A stream's client lives as long as the stream.** The client holds the call's registration in the
  room's shared response loop, so the forwarder moves it into a relay task that feeds the receiver it
  returns. The relay ends when the peer ends the stream, and also when the reader goes away: it watches
  `tx.closed()`, so a stream whose reader dropped is released even if the peer never sends again.
- **The reader must drain promptly.** The relay's channel is 32 frames deep and independent of the
  transport's own 32-frame channel. A reader that stalls backs the relay up, then that channel, and the
  room's shared response loop blocks for every call in flight. The kernel's relay drains into an
  unbounded buffer and never stalls; a second consumer must do the same.

The daemon's runtime builds the `CommonRoom` it hands `PeerRouting` through it, and the common-room
discovery loop builds its own the same way, over the slot they already hold.

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

## `LiveKitSessionReach` — opening the room for a reached session

`session_reach` implements [`SessionReach`](../../tddy-daemon-extensions/docs/extension-ports.md), the
port `ConnectSession` reports every reach of a session through. LiveKit is optional, so the room a
reach opens is an addition to a session and never a prerequisite for using it.

`LiveKitSessionReach::new(config, rooms, status)` takes the daemon's `DaemonConfig`, the shared
`SessionRoomRegistry` (the rooms it opens are the ones session deletion closes) and an
`IntegrationReporter`, and reports `livekit_integration_state(config.livekit)` immediately, so a
daemon with LiveKit configured is not `Disabled` merely because nothing has dialled it yet.

| Caller | What `session_reached` does |
|---|---|
| Arrived over LiveKit (`RequestTransport::LiveKit`) | Opens the room and exposes the terminal **now**, in the caller's time; a failure fails the request. A remote client needs the room to be answered at all. |
| Anyone else (`Direct`, `InProcess`, HTTP, a local socket) | Spawns the open in the background and returns `Ok(())`. Nothing on this host needs the room, so the caller never waits on, or fails with, LiveKit; the outcome lands on the status board and in a `warn` log line. |

**`OpenSessionRoom` is loud.** `session_room.SessionRoomService.OpenSessionRoom(session_token,
session_id)` is registered by the plugin (`session_room_entry`) and is what a client that is about to
join a session's room asks first: `tddy-session-sync` and `tddy-tools pty-relay --livekit-url`. It
resolves the session through `ReachableSessions::resolve`, which authenticates the token and refuses
a session this daemon does not facilitate, or one that records no checkout, with
`failed_precondition`; it then opens the room (or reuses the open one) and answers the room name
(`session-{id}`), the URL a client joins at and the daemon's server identity. A LiveKit failure fails
the call, naming the room; a daemon with no LiveKit credentials answers `failed_precondition`
("LiveKit is not configured on this daemon") rather than coordinates nothing serves.

### Status reporting

`livekit_integration_state(Option<&LiveKitConfig>)` is what the `livekit:` block alone says, before
anything is dialled: `Disabled` with no block, or with `enabled: false` and no credentials;
`Misconfigured { missing }` naming whichever of `url`, `api_key`, `api_secret` is blank, in that
order; otherwise `Connecting`. Every outcome of `open_now` reports on the same reporter:

| Outcome | Reported |
|---|---|
| Room opened or reused | `Connected` |
| No credentials (`Ok(None)`) | What the block alone says |
| `RoomOpenFailure::Server` (creating or joining the room on the LiveKit server failed) | `Unreachable { error }`, the same text the caller is given |
| `RoomOpenFailure::Elsewhere` (the session's services, its checkout, its terminal bridge, the session deleted mid-open) | Nothing: the request still fails, but the server was not the problem, so the entry is left alone |

`SessionRoomRegistry::ensure_open_staged` is `ensure_open` saying which stage failed; `ensure_open`
maps either variant to its `Status`.

The common-room discovery loop (`spawn_common_room_discovery_loop`) reports on the same reporter,
which the daemon clones into `PeerDiscoveryHandles.status`: the configuration's state while the first
attempt runs, `Connected` once the common room is joined, and `Unreachable { error }` for a failed
connect or a lost room. A failure that is really the configuration (`Disabled`, `Misconfigured`) is
reported as that. Retries do not report `Connecting` again, so an outage keeps the `since` of its
first failure. The room-opening reach and the loop are two connections writing one entry, last writer
wins: a session room that opens reports `Connected` while the common room is down.

An embedder that calls `runtime::spawn_common_room_discovery_task` has no board and reports on a
private one nobody reads.

## `LiveKitTerminalExposure` — serving a terminal in a room

Implements `TerminalExposure`. `from_config` reads the URL (`public_url` when the block has one, since the
participant is a client of the server), the API key and the secret from the `livekit:` block, and is `None`
without them. `expose` mints a participant token with `TokenGenerator` (24 h), joins `at.room` as
`at.identity` with `LiveKitParticipant::connect`, serves the terminal's RPC surface, and republishes the
session's metadata block through `spawn_session_metadata_republisher` at join and every 30 s, so a dropped
`set_metadata` or a rejoin does not lose the session's cross-host association. The returned `ExposedTerminal`
finishes when the room ends. The exposure does not mint through `RoomTokenMinter`.

## The direction was already right

`session_room` names four trait ports — `SessionTerminalBridge` and `WorktreeSource`, which it
defines, and `SessionTokenMinter` and `RemoteSnapshotSource`, which it re-exports from
`tddy_daemon_extensions::split_rooms` — and the core crates *implement* them.
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

## `LiveKitPeerTransport`: credential sync rides its own channel

`#keyring` 6/9's `tddy-credential-sync` defines `PeerTransport`, a port for advertising a daemon and
delivering sealed credential payloads to one peer; `LiveKitPeerTransport` implements it over this
crate's common room. See `docs/credential-sync.md` in `tddy-credential-sync` for the engine side —
the two admission checks, the wire format, reconciliation and the journal. This crate's part is
narrow and deliberately does not touch anything above:

- **Rides participant *attributes*, not `metadata`.** `metadata` is `livekit_peer_discovery`'s own
  single-writer channel (`DaemonAdvertisement`, republished on its eventual-consistency loop);
  writing a second thing into it would make this adapter a second writer racing the first.
  Attributes are untouched by everything else in this crate, so `advertise`/`peers` are additive.
- **Delivery rides the existing `forward_to_peer` RPC mechanism** (`peer_routing`'s `CommonRoom`),
  under its own service name (`CredentialSync`), the same data-channel path `ListProjects` and
  `StartSession` forwarding already use.
- **Bridges `PeerTransport`'s synchronous methods to LiveKit's async SDK** with
  `tokio::task::block_in_place` + `Handle::current().block_on` — needed on a multi-threaded runtime,
  which is every deployment this adapter ships in.
- **Does not change `ListEligibleDaemons` or `StartSession` forwarding**, or the trust model either
  keeps. Credentials get a second, independent gate; forwarding does not get a retrofit of it.

`tddy-daemon`'s `credential_sync` module constructs the real engine around this transport — its
identity verifier, its persisted transport key, and the peer-join trigger that calls `publish`. See
`tddy-credential-sync`'s own doc, § *Known limitations*, for what is not wired yet (the receiving
side's RPC registration, a trigger on vault change rather than only on join).

## Four edges had to be cut

Rust crates cannot be mutually dependent, and no crate here may reach `tddy-daemon` at all. Each cut
below spans a crate boundary only because this crate moves one end of it, and each direction was
chosen rather than defaulted.

| Edge | Cut, and why that direction |
|---|---|
| `livekit_peer_discovery` → `split_session::SPLIT_AGENT_IDENTITY_PREFIX` | the constant lives in `tddy_service::participant_identity`, beside the rule that refuses it, and `tddy_daemon_kernel::daemon_identity` re-exports it for the crates that mint agents' identities; `split_session` re-exports that. Defining it here instead would make the **producer** of the identity depend on the crate that *refuses* it, and would break again when `split_session` moves to a third crate |
| `common_room_supervisor` → `daemon_config_service` → `livekit_peer_discovery` | the `CommonRoomSupervisor` **trait** moves to `common_room_supervisor`, beside its one implementation, and `daemon_config_service` re-exports it. Re-pointing the return leg alone does not do it: the trait import is a `livekit → daemon` edge whether or not it closes a loop. The return leg was re-pointed too, from a re-export in a departing module to its real home, `tddy_daemon_kernel::daemon_identity::local_instance_id_for_config` |
| `session_room` → `session_attachments::list_session_attachments` | the listing is lifted to `tddy_workflow::artifact_paths`, beside the `session_attachments_root` that names the very directory it reads, and the daemon re-exports it. Not the kernel: this is a filesystem convention `tddy-workflow` already owns, not a daemon identity |
| `livekit_peer_discovery` → `oauth_loopback_tunnel` | at the split the tunnel went to `tddy-daemon-auth`, so the LiveKit crate would not reach the identity boundary. The tunnel has since moved back here as `oauth_loopback_tunnel`, with its metadata parser, because it is LiveKit work and the auth crate names no LiveKit crate; `spawn_common_room_discovery_task`, which composes it with the discovery loop, stays in `tddy-daemon`'s `runtime.rs` |

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
