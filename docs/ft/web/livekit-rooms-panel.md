# LiveKit Rooms & Participants Panel

A second panel on the **LiveKit** screen (`#/livekit`) that lists **every room on the LiveKit
server** and, under each, **the participants currently joined to it**. Hovering a participant
reveals that participant's raw metadata.

> **Relation to the existing panel:** the screen's existing **Connected participants** panel
> (see [web-terminal.md § Shared LiveKit room](./web-terminal.md#shared-livekit-room-livekitcommon_room))
> is unchanged. It stays on top, showing the participants of the **one** room this browser has
> joined, sourced from the LiveKit client SDK. The new **Rooms** panel is added **below** it and
> answers a different question — what else exists on the server. The common room therefore appears
> in both panels; that is intended, and the two are sourced independently.

## Motivation

The web can only see the room it joined. Everything else the system builds on LiveKit is invisible
from the dashboard: per-session terminal rooms (`SessionEntry.livekit_room`), PR-stack presenter
rooms, screen-share and VNC bridge rooms. When a terminal will not attach or a session's agent
appears dead, the first question an operator asks — *is anybody actually in that room?* — currently
requires shelling into the host and talking to the LiveKit server API by hand.

This panel makes the server's own view of rooms and presence a first-class dashboard readout.

## Placement

- Rendered inside `LiveKitAppPage` (`#/livekit`), **directly below** the existing
  `connected-participants-panel`, as a sibling panel `data-testid="livekit-rooms-panel"`.
- The screen keeps `AppShell variant="scroll"`; the rooms panel scrolls with the page.
- The existing panel is not modified — no columns move, and its inline **Metadata** column stays
  exactly as it is today. The hover affordance described below belongs to the **new panel only**.

## Displayed values

### Room rows

One row per room, sorted by room name, each carrying:

| Field | Description |
|-------|-------------|
| Name | The LiveKit room name (e.g. `livekit.common_room`, `daemon-pr-stack-presenter-room-0001`) |
| Label | A human name for the room, from a `label` string in the room's own metadata — shown beside the name when present, omitted entirely when not |
| Participants | Count of participants currently joined |
| Created | Room creation time, as reported by the server |

**On room labels.** Raw room names are opaque (`daemon-pr-stack-presenter-room-0001` does not say
"PR-stack presenter"), so `LiveKitRoomInfo` relays the room's own `metadata` — a field LiveKit's
server API already returns for every room — and the panel renders a `label` string out of it when
one is there. This is the channel by which room kinds can be distinguished.

**No publisher writes a `label` today**, so the label is normally absent. Room metadata itself is no
longer empty: a session room carries a worktree snapshot (`head_commit`, `branch`, `changed_paths`,
`changed_files`, `lines_added`, `lines_removed`, `untracked_files`, `attachments`,
`updated_at_unix_ms` — see [session-room.md](../daemon/session-room.md)), which has no `label` key.
The panel therefore treats the label as decoration: no behaviour keys off it, and a room whose
metadata carries none renders exactly as it would have without the field. Surfacing the worktree
snapshot itself in this panel is a separate, obvious next step, and deliberately not in scope here.

Room metadata is read at snapshot and on `room_added`; changes to it are not tracked as their own
change event, so a room's label and snapshot are as of when the panel first learned about the room.

Rooms render **expanded**, showing their participants; a row can be **collapsed** to hide them.
Expanded is the default because the panel exists to answer "who is in there" — starting collapsed
would put a click between the operator and the whole point of the screen. No room is special-cased:
every row behaves identically, so the panel needs no notion of which room is *the* common room.
Should a per-kind default ever be wanted, the room `label` described above is the hook for it.

### Participant rows

Nested under their room, sorted by identity:

| Field | Description |
|-------|-------------|
| Identity | The LiveKit participant identity |
| Role | `browser` / `coder` / `daemon` / `unknown`, inferred by the **existing** `inferParticipantRole` grammar (`src/lib/participantRole.ts`) so both panels label a participant the same way |
| Joined | When that participant joined the room |
| State | The server's participant state, relayed verbatim — `JOINING`, `JOINED`, `ACTIVE` or `DISCONNECTED`. The panel renders whatever string the server sends rather than validating against a list, so a state LiveKit adds later shows up rather than disappearing |

Metadata is **not** a column here. It is revealed on hover (below).

### Metadata on hover

Pointing at a participant row reveals that participant's **raw metadata JSON, pretty-printed**, in a
card anchored to the row (`data-testid="livekit-room-participant-metadata-{room}-{identity}"`).

- Metadata is a single JSON document **shallow-merged across independent publishers**, so the card
  shows the union of whatever that participant carries — `instance_id`/`label` (daemon
  advertisement), `owned_project_count`, `codex_oauth`, `session`. See
  [`participant-metadata.md`](../../../packages/tddy-livekit/docs/participant-metadata.md).
- A participant with **empty** metadata gets a card reading `No metadata published.` — the
  affordance is present either way, so "nothing published" is distinguishable from "did not point at
  the right thing".
- Metadata that is **not** valid JSON is shown verbatim rather than dropped; the daemon relays the
  string as published and the web does not validate it.
- Only one card is open at a time.
- Because a merged document can be long (the `session` block alone carries nine fields), the card is
  monospace with a bounded height and scrolls rather than growing without limit.

**The row is focusable, and keyboard focus opens the same card as the pointer.** The participant row
is a focusable element (`tabIndex=0`) wrapped in the app's existing Radix `Tooltip` primitive
(`src/components/ui/tooltip.tsx`), which opens on both `pointerenter` and `focus`. This is a
deliberate accessibility choice rather than an incidental one: metadata reachable only by pointer is
unreachable by keyboard. It also has a testing consequence worth stating plainly — the Cypress
component harness cannot synthesize the pointer events Radix listens for (the package carries no
`cypress-real-events`, and no spec in the repo triggers `mouseover`), so the acceptance tests drive
the card through **focus**. That is a real user path, not a stand-in for one; pointer hover is
verified by the shared Radix primitive, which is already exercised by the sessions drawer.

`LiveKitAppPage` has no `TooltipProvider` today; this feature adds one with `delayDuration={0}` so
the card appears without a dwell delay.

## RPC surface

A single server-streaming method on `ConnectionService`, addressed to the **selected daemon** over
the shared common-room LiveKit connection (no `daemon_instance_id` payload — the transport already
targets the daemon), exactly like `StreamHostStats`:

- `StreamLiveKitRooms(StreamLiveKitRoomsRequest) returns (stream LiveKitRoomsEvent)`

### Snapshot first, then changes

The **first** message on the stream is always a **full snapshot** of current state. **Every
subsequent** message is a **change event** describing one delta. The web applies the snapshot as
its whole state and then folds each change into it; it never re-requests the full list.

```
LiveKitRoomsEvent {
  oneof event {
    LiveKitRoomsSnapshot snapshot;   // first message only
    LiveKitRoomsChange   change;     // every message after it
  }
}
```

Change events:

| Event | Carries | Meaning |
|-------|---------|---------|
| `room_added` | the full `LiveKitRoomInfo`, participants included | a room appeared (possibly already occupied) |
| `room_removed` | room name | the room closed |
| `participant_joined` | room name + full `LiveKitParticipantInfo` | someone joined an already-known room |
| `participant_left` | room name + identity | someone left |
| `participant_metadata_changed` | room name + identity + new metadata | a publisher re-published metadata |
| `participant_state_changed` | room name + identity + new state | the server moved an already-known participant to another state, e.g. the `JOINED` → `ACTIVE` settle after a join |

A change naming a room the client does not know is **ignored** rather than fabricating a room from
a partial event; the next `room_added` carries the authoritative row.

Metadata and state are diffed independently, so a tick that moved both carries **two** events — one
delta per event, as everywhere else on this feed — rather than one combined frame.

### The daemon owns the cadence

The LiveKit server API offers no change feed, so the daemon polls it — `list_rooms`, then
`list_participants` per room — **every 3 seconds**, diffs the result against the state it last sent
on that stream, and emits **one change event per delta**. A tick with no delta emits nothing, so an
idle system produces an idle stream. Presence is the volatile fact on this screen, which is why the
cadence is faster than the host-stats disk tick.

Each subscription keeps its **own** last-sent state, so two watchers cannot desynchronize each
other — and its own poller. The poll cost is therefore **`1 + room count` calls every 3 seconds per
open subscription**, not per daemon.

Room count is no longer small. Every agent session now gets a LiveKit room of its own (see
[session-room.md](../daemon/session-room.md)), so a daemon with thirty live sessions serves ~31 calls
per tick **per open panel** — two operators watching is ~62 calls every 3 seconds. That is the price
of the per-subscriber baseline. It is bounded by the number of panels actually open, because the poll
loop ends with its subscriber (below), and it is correct at any scale — but coalescing onto one
shared poller that broadcasts full rosters, with each subscriber diffing locally, would preserve the
same guarantee at one poller per daemon. Given the room count now scales with session count rather
than with a handful of fixed rooms, that is less "an escape hatch if this becomes commonly-open" than
the expected next change.

A read of the roster is bounded by a **5-second deadline**. A server API that accepts the read and
never answers would otherwise stall the stream with no error frame, leaving the panel on a stale
roster that looks healthy; expiry takes the same error path an outright failure takes.

**The poll loop lives and dies with its subscriber.** Because an idle stream sends nothing, a loop
that learned about departure only from a failed send would never learn at all — so it watches for the
subscriber going away directly. Without that, every visit to this screen left a permanent 3-second
poll of the LiveKit server behind it.

`StreamLiveKitRoomsRequest` carries a `session_token`; an invalid token is rejected with an
unauthenticated error, like every other `ConnectionService` method.

### Where the room facts come from

The daemon already holds the LiveKit **API key, secret, and URL** (`DaemonConfig.livekit` —
`api_key`, `api_secret`, `url`) — it mints join tokens with them via `TokenService`. The same
credentials drive a LiveKit **server-API** room client — the same API surface `RoomMetadataClient`
uses to publish a session room's worktree snapshot, reached here for reading rather than writing.
It introduces **no new dependency**: `livekit-api` is already a normal dependency of `tddy-livekit`,
which uses its `access_token` module to mint tokens.

Two details the implementation must get right:

- **The configured URL is `ws://`/`wss://`; the room client needs `http(s)`.** No production crate
  derives one from the other — the only converter is test-only and requires an explicit port, so a
  portless `wss://host` would not survive it. A small derivation helper belongs in `tddy-livekit`
  beside `token.rs`, handling the portless case and the implicit-scheme-port mapping.
- **Use `livekit.url`, not `livekit.public_url`.** The latter is the browser-facing address; the
  daemon reaches the server API over its own (possibly internal) URL.

If the daemon cannot reach the LiveKit server API, the stream fails with that error and the panel
shows it (below). It does not fall back to the browser's client-SDK view of the one room it joined —
that would silently answer a different question than the one asked.

## States

| State | Rendering |
|-------|-----------|
| Before the first snapshot | `Loading rooms…` |
| Snapshot with no rooms | `No rooms on the LiveKit server.` |
| Stream error | the error the daemon reported, in the panel (`data-testid="livekit-rooms-panel-error"`) |
| Known room with no participants | the room row renders with a count of `0` and says `No participants joined.` |

A stream error after a snapshot has arrived **keeps the last-known rooms on screen** alongside the
error — a stale roster plus a visible error beats an empty panel.

## Acceptance criteria

1. `#/livekit` renders a **Rooms** panel below the existing **Connected participants** panel, and
   the existing panel's markup and columns are unchanged.
2. The panel lists one row per room from the stream's first message, each showing the room name, its
   participant count, and its creation time.
2a. A room whose metadata carries a `label` shows that label beside its name; a room whose metadata
   carries none shows no label element at all.
3. Each room row lists its participants with identity, role, joined time, and server state; the role
   uses the same inference grammar as the existing participants panel. Collapsing a room row hides
   its participants.
4. Pointing at — or keyboard-focusing — a participant row reveals a card containing that
   participant's pretty-printed metadata JSON.
5. The same affordance on a participant that published no metadata reveals a card stating no
   metadata is published, rather than no card at all.
6. The panel applies the first stream message as a full snapshot and each later message as a single
   change event — a `participant_joined` event adds exactly that participant to exactly that room
   without re-requesting the list.
7. A `participant_left` event removes that participant and decrements the room's count; a
   `room_removed` event removes the room.
8. A `participant_metadata_changed` event updates what that participant's hover card shows.
8a. A `participant_state_changed` event updates that participant's **State** cell, so a subscriber
   that was already watching sees the `JOINED` → `ACTIVE` settle rather than the state the
   participant was first seen in.
9. The web opens exactly **one** `StreamLiveKitRooms` subscription for the screen, and cancels it on
   unmount.
10. Switching the selected daemon re-subscribes and re-renders the room list for the newly selected
    daemon.
11. A stream error before any snapshot shows the daemon's error in the panel; a stream error after a
    snapshot keeps the last-known rooms visible alongside the error.
12. `StreamLiveKitRooms` rejects an invalid session token.
13. The daemon emits nothing on a poll tick that produced no delta.

## Related documentation

- **[web-terminal.md § Shared LiveKit room](./web-terminal.md#shared-livekit-room-livekitcommon_room)** — the existing presence panel this one sits below
- **[livekit-participant-owned-projects.md](./livekit-participant-owned-projects.md)** — the `owned_project_count` metadata key and the existing Metadata column
- **[host-stats-footer.md § RPC surface](./host-stats-footer.md#rpc-surface)** — the streaming-readout pattern this RPC follows
- **[`participant-metadata.md`](../../../packages/tddy-livekit/docs/participant-metadata.md)** — what participant metadata carries
- **[app-shell.md](./app-shell.md)** — where the `#/livekit` screen is registered
