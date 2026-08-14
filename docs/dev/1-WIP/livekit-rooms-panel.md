# Changeset: LiveKit rooms & participants panel

**PRD**: `docs/ft/web/livekit-rooms-panel.md`
**Branch**: `feat-ui-rooms-participants`

A second panel on `#/livekit` listing every room on the LiveKit server and the participants joined to
each, fed by a new `ConnectionService.StreamLiveKitRooms` server-stream whose first message is a full
snapshot and whose every later message is one change event. Participant metadata is revealed on
pointer-hover or keyboard focus.

## Checklist

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Add `StreamLiveKitRooms` + messages to `connection.proto`, regenerate TS
- [x] Write acceptance tests (Cypress component) — 22, all failing
- [x] Write unit/integration tests — 19 `bun test`, 16 Rust, all failing but the auth test
- [x] tddy-daemon: tonic adapter method (mirrors the rpc-flavor handler)
- [x] tddy-livekit: `http_base_from_ws_url` — 7 unit tests green
- [x] tddy-livekit: `LiveKitRoomRoster` — server-API listing behind the `RoomRoster` trait
- [x] tddy-daemon: `diff_rosters` — 9 unit tests green
- [x] tddy-daemon: `stream_live_kit_rooms` handler — poll, diff, emit
- [x] tddy-daemon: inject `RoomRoster` + poll interval into `ConnectionServiceImpl`
- [x] tddy-web: `liveKitRoomsState` reducer — 19 unit tests green
- [x] tddy-web: `useLiveKitRooms` streaming hook
- [x] tddy-web: `LiveKitRoomsPanel` component + `TooltipProvider` on `LiveKitAppPage`

## Files to create

| File | Purpose |
|------|---------|
| `packages/tddy-livekit/src/server_api_url.rs` | `http_base_from_ws_url(&str) -> Result<String>` — `ws://`→`http://`, `wss://`→`https://`, implicit ports preserved, portless hosts accepted |
| `packages/tddy-livekit/src/room_roster.rs` | `LiveKitRoomRoster` — `list_rooms()` → `Vec<LiveKitRoomInfo>` via `livekit_api::services::room::RoomClient` (`list_rooms` + `list_participants` per room). The `RoomRoster` **trait** stays in `tddy-daemon`, which owns the handler; the daemon implements it for this type |
| `packages/tddy-livekit/tests/room_roster_livekit.rs` | Integration: the mapping against a real LiveKit server (testkit) — identity, metadata and state as the server reports them |
| `packages/tddy-livekit/tests/room_roster_deadline.rs` | Integration: a roster read against a server API that accepts and never answers gives up on its deadline |
| `packages/tddy-daemon/src/livekit_rooms_stream.rs` | Poll/diff engine: `diff_rosters(prev, next) -> Vec<LiveKitRoomsChange>`, and the 3 s sampling loop feeding one subscriber |
| `packages/tddy-web/src/lib/liveKitRoomsState.ts` | Pure state: `LiveKitRoom`/`LiveKitRoomParticipant` types, `roomsFromSnapshot`, `applyRoomsChange` |
| `packages/tddy-web/src/lib/liveKitRoomsState.test.ts` | Unit tests for the reducer (`bun test src/lib`) |
| `packages/tddy-web/src/rpc/useLiveKitRooms.ts` | `useLiveKitRooms()` — one `StreamLiveKitRooms` subscription (abortable, cancelled on unmount), folds snapshot + changes, exposes `{ rooms, hasSnapshot, error }` |
| `packages/tddy-web/src/lib/testId.ts` (+ test) | `safeTestIdPart` — one copy, shared by both panels and the Cypress test-id helpers, so a component and its selectors cannot drift |
| `packages/tddy-web/src/lib/liveKitMetadataCard.ts` (+ test) | `metadataCardText` — pretty-printed JSON / verbatim string / "no metadata published" |
| `packages/tddy-web/src/components/livekit/LiveKitRoomsPanel.tsx` | The panel: room rows, expand/collapse, participant rows, metadata tooltip |
| `packages/tddy-web/cypress/support/rpc/liveKitRoomsBackend.ts` | `aLiveKitRoomsBackend(scenario)` → `{ backend, roomsStreamCount(), pushChange(change) }` |
| `packages/tddy-web/cypress/support/pages/liveKitRoomsPanelPage.ts` | Page object |
| `packages/tddy-web/cypress/component/LiveKitRoomsPanelAcceptance.cy.tsx` | Acceptance tests |

## Files to modify

| File | Change |
|------|--------|
| `packages/tddy-service/proto/connection.proto` | `rpc StreamLiveKitRooms`; messages `StreamLiveKitRoomsRequest`, `LiveKitRoomsEvent`, `LiveKitRoomsSnapshot`, `LiveKitRoomsChange`, `LiveKitRoomInfo`, `LiveKitParticipantInfo`, `LiveKitRoomAdded/Removed`, `LiveKitParticipantJoined/Left/MetadataChanged/StateChanged` |
| `packages/tddy-web/src/gen/connection_pb.ts` | Regenerated (`bun run generate`) |
| `packages/tddy-livekit/src/lib.rs` | Export `room_roster` and `server_api_url`. No `Cargo.toml` change anywhere: the room client lives in `tddy-livekit`, which already depends on `livekit-api` |
| `packages/tddy-daemon/src/connection_service.rs` | `StreamLiveKitRoomsStream` assoc type + `stream_live_kit_rooms` handler; `room_roster: Arc<dyn RoomRoster>` + poll interval on the impl struct, `with_room_roster` / `with_room_poll_interval` test builders |
| `packages/tddy-daemon/src/connection_tonic_adapter.rs` | Mirror the method (mandatory — the local UDS server implements the tonic trait) |
| `packages/tddy-codegen/src/generator.rs` | The generated server-streaming pump breaks when its send fails, instead of discarding the error — shared by every server-streaming RPC |
| `packages/tddy-web/src/components/livekit/LiveKitAppPage.tsx` | Render `<LiveKitRoomsPanel />` below the existing panel; wrap in `TooltipProvider delayDuration={0}` |
| `packages/tddy-web/cypress/support/testIds.ts` | Panel ids + `livekitRoomEntry(room)`, `livekitRoomParticipantEntry(room, identity)`, … |
| `docs/ft/web/changelog.md`, `docs/dev/changesets.md` | On wrap |

## Design decisions

### Snapshot first, then one change per delta
The stream's first message is a `LiveKitRoomsSnapshot`; every later message is a `LiveKitRoomsChange`
carrying exactly one delta. The client never re-requests the list. This keeps steady-state traffic
proportional to *churn* rather than to roster size — a 20-room server with nobody joining or leaving
pushes nothing at all.

A change naming an unknown room is **dropped**, not used to conjure a room from partial data: only
`room_added` carries an authoritative row (name, creation time, full participant list).

The six change kinds are `room_added`, `room_removed`, `participant_joined`, `participant_left`,
`participant_metadata_changed` and `participant_state_changed`.

### A participant's metadata and state are separate deltas
`diff_rosters` compares both for a participant present in two consecutive readings, and a tick that
moved both emits **two** events rather than one combined frame — the feed's contract is one delta per
event, so each fact travels on the event named for it and neither is hidden behind the other's
arrival. Without the state comparison a participant already known to a stream never got a state
update at all: its **State** cell showed whatever it was first seen as, so the `JOINED` → `ACTIVE`
settle that follows every join was invisible to anyone already watching.

### The daemon owns the cadence; each subscriber owns its diff baseline
The LiveKit server API has no change feed, so the daemon polls (`list_rooms`, then
`list_participants` per room) every 3 s and diffs against **the state it last sent on that
stream**. Per-subscriber baselines mean two watchers cannot desynchronize each other — a shared
baseline would let watcher B's tick consume the delta watcher A had not yet been sent.

Presence is the volatile fact here, hence 3 s rather than the host-stats disk tick's 60 s.

The cost of that choice, stated plainly: each subscription runs its own poller, so the load is
`1 + room count` calls every 3 s **per open subscription**, not per daemon. Five watchers on a
twenty-room server is 105 calls per tick. It is bounded by panels actually open (the loop ends with
its subscriber), and acceptable at current scale. One shared poller broadcasting full rosters, with
each subscriber diffing locally, would preserve the per-subscriber baseline at one poller per daemon
— the escape hatch if this panel becomes commonly-open.

### The poll loop lives and dies with its subscriber
`pump_rooms` (in `livekit_rooms_stream.rs`) selects on `tx.closed()` alongside the tick, so a
subscriber that goes away ends the loop *even while the roster is idle* — an idle stream sends
nothing, so a send failure would never be reached to notice. Without it every subscription left a
permanent 3 s poll of the LiveKit server behind it.

That only works because the transport propagates the teardown: the generated server-streaming pump
(`tddy-codegen`) now stops draining a handler's stream once its own send fails, instead of discarding
the error and pulling items into a void forever — which had been holding every handler's stream, and
therefore its "my subscriber left" check, alive.

Ticks are `MissedTickBehavior::Skip`: a read slower than the cadence must not queue up the ticks it
outlasted, since bursting them fires another `1 + rooms` calls at exactly the moment the server API
is already slow.

### A roster read has a deadline
`LiveKitRoomRoster::list_rooms` bounds the whole read (`ListRooms` plus the per-room
`ListParticipants`) at 5 s. A server API that accepts the read and never answers would otherwise stall
the stream indefinitely with no error frame, leaving the panel on a stale roster that looks healthy.
Expiry takes the same error path a failed read takes. The ceiling is per read rather than per call, so
it does not scale with the room count, and it sits far above a healthy read (single-digit ms on a LAN)
so ordinary slowness is not reported as breakage.

### A configuration gap is a `FAILED_PRECONDITION`, a read failure is `INTERNAL`
`RoomRoster` returns `RosterError::{Unconfigured, ReadFailed}` rather than a bare string, so the two
reach the subscriber as different status codes. Missing credentials or a `livekit.url` that is not a
WebSocket address is this daemon's deployment gap — retrying cannot fix it — while an unreachable or
failing server API is not.

### Room listing lives in `tddy-livekit`, behind a trait
`RoomRoster` is a trait with a `LiveKitRoomRoster` implementation, mirroring how `HostStats` is
injected into `ConnectionServiceImpl` for `StreamHostStats`. That is what lets the daemon handler
tests drive a scripted roster sequence without a LiveKit server. `livekit-api` is already a normal
dependency of `tddy-livekit`, so no new crate enters the tree.

The production roster is built inside `ConnectionServiceImpl::new` from the `DaemonConfig` it already
receives (as `host_stats` is), not at the `main.rs` call site: every transport shares one instance
built there, so wiring it at the construction site cannot be forgotten by a second call site. A
daemon without all three of `livekit.url` / `api_key` / `api_secret` gets a roster that reports that
gap — the stream fails with the reason instead of reporting an empty server.

### Room labels ride on room metadata
`LiveKitRoomInfo.metadata` relays the room's own metadata — already returned by LiveKit's server API
for every room, so carrying it costs one field and no new call. The panel renders a `label` string
from it beside the opaque room name. Nothing publishes room metadata today, so this plumbs the
channel rather than lighting it up: the label is decoration, no behaviour keys off it, and a room
without one renders as if the field did not exist. That keeps it from being a fallback — there is
nothing to fall back *to*, and its absence changes nothing.

### No fallback to the browser's client-SDK view
If the server API is unreachable the stream errors and the panel says so. Substituting
`useRoomParticipants`' view of the one joined room would answer a different question under the same
heading. Per the repo's no-fallbacks rule.

### A stale roster beats an empty one
A stream error *after* a snapshot keeps the last-known rooms rendered alongside the error. Only an
error *before* any snapshot leaves the panel with nothing but the message.

### Metadata card on hover **and** focus
The participant row is focusable and wrapped in the app's existing Radix `Tooltip`, which opens on
`pointerenter` and on `focus`. Two reasons, one of them a constraint:

- Metadata reachable only by pointer is unreachable by keyboard.
- The Cypress component harness cannot synthesize Radix's pointer events — the package carries no
  `cypress-real-events` and no spec in the repo triggers `mouseover`. The specs therefore drive the
  card through focus, which is a real user path rather than a stand-in for one. The repo's only
  existing tooltip spec (`SessionsDrawerAcceptance.cy.tsx:219`) does the same.

`LiveKitAppPage` has no `TooltipProvider`; this adds one with `delayDuration={0}`.

### Stream subscriptions are tallied by the fake
`anInMemoryRpcBackend` records unary requests only (its interceptor skips `req.stream`), so
"exactly one subscription" is asserted through a counter the fake keeps itself — the
`hostStatsStreamCount()` convention already in `connectionServiceBackend.ts`.

## Acceptance tests

`packages/tddy-web/cypress/component/LiveKitRoomsPanelAcceptance.cy.tsx` — one behavior per test,
mounting `LiveKitAppPage` directly with the rooms backend:

| # | Test | PRD criterion |
|---|------|---------------|
| 1 | renders the rooms panel below the connected-participants panel | 1 |
| 2 | lists one row per room from the stream's first message, with name, participant count and creation time | 2 |
| 2a | names a room by the label in its metadata | 2a |
| 2b | shows no label for a room whose metadata carries none | 2a |
| 3 | reveals a room's participants with identity, role, joined time and server state when the row is expanded | 3 |
| 4 | labels a participant with the same role the connected-participants panel infers | 3 |
| 5 | reveals a participant's pretty-printed metadata when the row takes focus | 4 |
| 6 | states that no metadata is published for a participant that published none | 5 |
| 7 | adds a joining participant to its room without re-requesting the list | 6, 9 |
| 8 | removes a leaving participant and decrements its room's count | 7 |
| 9 | removes a room on `room_removed` | 7 |
| 10 | updates a participant's metadata card on `participant_metadata_changed` | 8 |
| 10a | updates a participant's state cell on `participant_state_changed` | 8a |
| 11 | ignores a change event naming a room it does not know | 6 |
| 12 | says the server has no rooms when the snapshot is empty | states |
| 13 | says a known room has no participants when it is empty | states |
| 14 | shows the daemon's error when the stream fails before any snapshot | 11 |
| 15 | keeps the last-known rooms visible when the stream fails after a snapshot | 11 |
| 16 | opens exactly one rooms subscription for the screen | 9 |

Criterion 10 (re-subscribe on daemon switch) is covered by the hook's `[client, sessionToken]`
dependency, shared with `useHostStats`, and by `DaemonChangeReloadsScreenAcceptance`-style coverage —
not re-tested per-panel.

## Unit / integration tests

**`packages/tddy-web/src/lib/liveKitRoomsState.test.ts`** (`bun test src/lib`) — the reducer in
isolation: snapshot → rooms; each of the six change kinds; unknown-room drop; participant ordering
by identity; room ordering by name; count derived from the participant list rather than carried
separately.

**`packages/tddy-daemon/src/livekit_rooms_stream.rs`** (Rust unit) — `diff_rosters`:
room added/removed, participant joined/left, metadata changed, no-delta tick emits nothing, a
`JOINED` → `ACTIVE` settle emits `participant_state_changed`, a tick moving both metadata and state
emits one event for each, a participant re-reported unchanged emits nothing, and a simultaneous
multi-delta tick emits one event per delta. Plus `room_roster_from_config`: all three
credentials yield the server-API reader, each one missing yields the configuration reason, and a
non-WebSocket `livekit.url` yields the URL error as that reason.

**`packages/tddy-livekit/src/server_api_url.rs`** (Rust unit) — `ws://h:7880` → `http://h:7880`,
`wss://h:443` → `https://h:443`, portless `wss://h` → `https://h`, path preserved, non-ws scheme
rejected.

**`packages/tddy-daemon/tests/stream_livekit_rooms_rpc.rs`** — the handler's contract, driven by a
scripted `RoomRoster` and a 20 ms injected poll interval: an invalid session token is refused before
any stream opens; the first message is a snapshot of every room; the message after it carries exactly
the one delta the next reading introduced; a reading that did not move keeps the stream silent *while
the roster is provably still being read*; a dropped stream stops the reads; a roster read that fails —
on the first poll or a later one — ends the stream with that error and `INTERNAL` instead of an empty
room list; and an unconfigured daemon ends it with `FAILED_PRECONDITION`.

**`packages/tddy-livekit/tests/room_roster_livekit.rs`** — what only a live server can pin: that
`ListRooms`/`ListParticipants` fields land in the wire types (identity and metadata verbatim, and a
connected participant reaching the panel as `ACTIVE`). Runs against the testkit container.

**`packages/tddy-livekit/tests/room_roster_deadline.rs`** — a loopback listener that accepts the read
and answers nothing: the roster reports the expiry rather than stalling. No LiveKit server needed.

## Out of scope

- Room *actions* (kick a participant, delete a room) — read-only panel.
- Webhook-driven presence instead of polling; the daemon is not configured to receive LiveKit
  webhooks.
- Reconciling the two panels into one. The PRD keeps them independent and deliberately duplicated.
