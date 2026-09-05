# Capability gating (`src/rpc/connections/useHasCapability.ts`, `src/hooks/capabilityAvailability.ts`)

Most of what `tddy-web` shows a host is plain RPC, and a [host connection](host-connections.md)
carries that over whatever wire it was opened on. Tracks and presence are the exception: a video
frame and a participant roster are things a wire either carries or does not, and no abstraction
turns a frame pipe into a media server.

So the media and presence surfaces are **gated**. Each renders only when the connection in scope
advertises the capability it needs, and says why when it does not.

Feature docs: [Daemon selector + host-connection routing](../../../docs/ft/web/daemon-selector-livekit-rpc.md),
[App shell](../../../docs/ft/web/app-shell.md), [Session drawer](../../../docs/ft/web/session-drawer.md),
[LiveKit rooms panel](../../../docs/ft/web/livekit-rooms-panel.md).

## The one predicate

`useHasCapability(connection, capability)` — and its non-hook twin `hasCapability` — is the only
place a `capabilities` set is read. Nothing re-derives the answer from a transport, from a status
string, or from the presence of a `Room`.

That is a deliberate singularity rather than tidiness. Capability information reaches the app from
three directions — a host connection's `capabilities`, a session connection's, and
`capabilitiesForHint` behind both — and a fourth place that answered "can I show video here" its own
way is precisely the drift that would make the wire-neutral model decorative.

A `null` connection answers `false`. Nothing is selected, or nothing can reach it, and either way
there is no surface to show. A caller that needs to tell "no host" from "host without video" is
asking a routing question, not a capability one, and reads `status`.

## Two facts, in one order

**A capability alone cannot gate anything.** A common room that is still joining has not yet
produced a connection carrying anything — the LiveKit provider is bound to a `null` room until
`Room.connect()` resolves — and a join that failed produces no connection at all. A surface that
asked the capability first would therefore announce "not available on this connection" for the
second or two every LiveKit page spends connecting, and then contradict itself.

Both halves of that had a concrete symptom before the rule existed:

- **Presence** — the verdict would have replaced the reason a join failed. That reason reaching the
  participant panel is the whole point of the 2026-08-13 `udoo` incident, where ICE never
  established and every symptom an operator could see was indistinguishable from "still
  connecting"; it is pinned by `CommonRoomConnectionVisibilityAcceptance`.
- **Media** — the session inspector's tab strip rendered seven tabs on load and nine a second later,
  reflowing under the operator's cursor on every LiveKit page load.

`capabilityAvailability(roomStatus, hasCapability)` (`src/hooks/capabilityAvailability.ts`) is the
resulting order, stated once:

| Verdict | When | What the surface does |
|---|---|---|
| `error` | the join failed | keep the surface, and quote the reason |
| `connecting` | the join is in flight | keep it, say what it is waiting on, claim nothing else |
| `unavailable` | nothing is being joined and the wire carries no such capability | name that as the reason it is not there |
| `available` | otherwise | render as normal |

Status answers *for a capability that exists*; the predicate answers *whether it exists at all*. Read
in that order a navigation entry and the screen it points at cannot disagree, and a failed join keeps
both — because the reason it failed is what an operator would go there to find.

Treating `connecting` as "keep it" is safe in the other direction too. A host that never joins a room
— a build reaching its own daemon over an in-process bridge — reports `idle` and never `connecting`,
so nothing appears there only to vanish.

**`available` is a verdict about the capability, not about the join.** A wire that carries presence
with no join in flight reaches `available` from `idle` just as readily as from `connected`, so a
surface that reports the room's state reports `roomStatus` itself rather than stamping `connected`.
`ParticipantList`'s `data-room-status` is the case that matters: collapsing `idle` and `connecting`
into one "Connecting…" branch used to make the attribute true by construction, and letting `idle`
fall through to a branch hard-coded to `connected` had the DOM claim a room was joined that nothing
had joined.

### `useCapabilityAvailability` is how a surface asks

Centralising the rule stopped the surfaces drifting, but the drift surface had merely moved up a
level: six components still re-assembled the same three lines around it — resolve the connection,
read the common room out of the host directory, apply `useHasCapability`, default a missing source to
`idle`. Six copies of an argument list are six chances to pass the wrong capability, read a different
source, or default the missing status the other way.

`useCapabilityAvailability(connection, capability)` (`src/hooks/useCapabilityAvailability.ts`) is now
the only place any of that happens, `?? "idle"` included. An absent LiveKit source is "no common room
was ever asked for" — the desktop build, and every component test that mounts no directory sources —
and reading it as anything else would invent either a join in flight or a failure. A call site states
only what is genuinely its own: which connection, and which capability.

It takes a connection rather than a host id because `SessionInspectorDrawer` is *handed* one as a
prop and has no id to resolve from; a hook keyed on an id would have left that site assembling the
answer by hand, which is the thing being removed.

The rule itself stays a pure function with no React and no host-directory import, which is what lets
its truth table be stated in a unit test without rendering anything.

`ParticipantList` is the one surface that calls the pure rule directly. It is presentational and is
*told* which room's status it is reporting on, so the screen above it owns that half.

## Which connection answers

**The host connection gates the media surfaces**, not the session's, even for surfaces that live
inside a session.

- `capabilitiesForHint` derives a session's capabilities from whether its attach hint names a room,
  and whether there is a room is decided by how the *host* is reached. Host capability is therefore
  the upstream fact: a host reached without LiveKit can never hand out a room-backed session.
- **A dormant session has no session connection at all.** `SessionsDrawerScreen` attaches only to an
  active session, so reading a session connection there answers `false` to a question that is
  actually unanswerable — which is not gating. It hides the media tabs on every dormant session.

The session connection is read in exactly one media decision, `SessionRuntime`'s `carriesMedia`,
which chooses the terminal component. That one genuinely is session-scoped: a session connection
exists only once the session is attached, and attaching is not a common-room join.

## Hide, don't disable

A gated surface is **removed from navigation**, not rendered disabled. A tab the user cannot use is
worse than a tab that is not there: a disabled VNC tab invites a support question with no good
answer, while an absent one matches the operator's model — this host is reached a way that has no
video.

Where an entry point must stay for layout reasons, it carries an explicit explanation naming the
reason instead of vanishing: "not available on this connection: this host is reached over a wire that
carries no LiveKit presence".

| Surface | Gated on | Absent state |
|---|---|---|
| `ParticipantList` | host connection + common-room status | names the connection as the reason |
| `LiveKitRoomsPanel` — the panel | host connection + common-room status | removed; while the join is in flight or failed the panel keeps its place and says what it is waiting on |
| `LiveKitRoomsPanel` — its `StreamLiveKitRooms` feed | host connection alone | never subscribed |
| `LiveKitAppPage` | host connection + common-room status | the route stays reachable and explains itself |
| `DaemonNavMenu`'s LiveKit entry | host connection + common-room status | removed from the menu |
| `RpcPlaygroundScreen`'s participant picker | host connection + common-room status | replaced by the reason there is nobody to address |
| `SessionsDrawerScreen` cross-host rows | host connection + common-room status | `ListSessions` rows plus a footnote naming what is out of view |
| `SessionInspectorDrawer`'s VNC and Screen Sharing tabs, panel dispatch and `?inspector=` fallback | host connection + common-room status | removed from the strip; a media tab named in the URL degrades to Details |
| `ParticipantVideoPreviewDialog` and the roster's camera column | host connection, bare predicate | no camera affordance in the roster |

**Whether a panel applies and whether its feed is opened are two questions, and they get two gates.**
`LiveKitRoomsPanel` keeps its frame through a join in flight, so the page does not reflow when the
room list drops in; but `useLiveKitRooms` subscribes from an effect the moment the feed mounts, and a
`StreamLiveKitRooms` the daemon has to serve for a panel nobody can read is a call made for nothing.
The feed is therefore a child component mounted only on `available` — the hook cannot be skipped by
the component that decides whether the panel applies.

Two media reads deliberately stay on the bare predicate. The roster's camera column sits *inside* the
rendered roster, which exists only once presence has resolved, so there is no strip to reflow; and
`SessionRuntime`'s `carriesMedia` is session-scoped, as above.

## Absence is stated, never defaulted

`ParticipantList.connection`, `RpcPlaygroundScreen.presenceAvailable`,
`SessionDrawer.crossHostSessionsVisible`, `SessionInspectorDrawer.hostConnection`,
`SessionMainPane.host` and `InspectorTabs.mediaAvailable` are **required** props with no default.

A default would mean "unknown ⇒ show it", which is the silence this gating exists to remove: a list
that has lost rows looks exactly like a list that never had them. Defaulting the other way (`= null`,
meaning "no media") is worse in the same manner — a call site that simply forgot the prop would lose
the VNC and Screen Sharing tabs silently and look exactly like a host that cannot carry a track.
Every mounting site therefore names the connection it is mounting with.

## Current limits

- **The status half is fleet-wide; the capability half is one host's.** There is one LiveKit host
  directory source per page — the common room either joined or it did not — while the capability
  belongs to one host's connection. So while the common room is `connecting`, or permanently in
  `error`, the rule answers "not unavailable" for *every* host, including one reached over a wire
  that provably cannot carry a track.

  Scoping the status per host does not work today. A host's own `status` cannot supply it: the
  LiveKit provider returns no connection until it has a room, so during the very window the rule
  exists to cover there is no `HostConnection` to read a status — or a capability — off at all, and
  that absence *is* the fact the common-room source stands in for. Nor can the descriptor's source
  id: it names which source *advertised* the host first, not which wire would carry the capability,
  and the two differ for the case that matters — the serving daemon is advertised by the serving
  source and is simultaneously reachable over LiveKit, so reading the serving source's status would
  make the media tabs vanish and return on every LiveKit page load, which is the reflow the rule was
  added to stop.

  It is acceptable while the fleet is single-wire: the LiveKit provider is the only registered
  provider, so every host connection answers the capability question identically and a fleet-wide
  status cannot contradict a per-host one. The surfaces that stay visible under `connecting` and
  `error` also explain themselves. **It stops being acceptable at the first mixed fleet** — a build
  that registers an in-process provider alongside LiveKit, where one host is reached over IPC and
  another over a room. There, a common room stuck in `error` would keep the media tabs on a host that
  can never serve a track. The fix belongs in `useCapabilityAvailability`, which is the other reason
  that hook exists; what it needs is a way to ask which source would carry a given capability for a
  given host, and only a second registered provider is in a position to define that.

- **Gating is about rendering, not about attaching.** Whether a session pane is a workflow view, and
  whether a terminal claim is offered, are unaffected by capability.

- **The input half of a remote desktop is ordinary RPC.** VNC and screen sharing each split into a
  video track and an input stream, and only the track needs a media wire. The whole tab is gated
  because a remote desktop with input and no picture is not a feature, but a later transport that
  carries frames some other way could revisit the split.
