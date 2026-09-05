# Session connections (`src/rpc/connections/session.ts`, `livekit/`)

A [host connection](host-connections.md) reaches a daemon. A **session connection** reaches one
session running on one of those daemons, and it is what `HostConnection.openSession(sessionId, hint)`
returns.

Attaching to a session is where the most wire-specific machinery used to live: a *second* LiveKit
room join per attached session, an observer identity minted for it, a browser token generated and
refreshed on a TTL timer, and RPC addressed at the session process's own participant
`daemon-<instance>-<session>`. Where the daemon's attach reply named no room, none of that happened
and session RPC quietly used the daemon's own client instead. Those were two attachment statuses —
`connected-livekit` and `connected-grpc` — and every consumer branched on them, so a third wire would
have meant a third branch in every one.

A `SessionConnection` is both of those, named once. What opening one costs — a room, a channel,
nothing at all — is the provider's business.

Feature docs: [Session drawer](../../../docs/ft/web/session-drawer.md).

## The model

| Type | What it is |
|---|---|
| `SessionConnection` | A live connection to one session on one host: `hostId`, `sessionId`, `status`, `error`, `capabilities`, `clientFor(service)`, `transport()`, `close()` |
| `SessionAttachmentHint` | What the attach reply said about reaching this session, in transport-neutral terms: `sessionId`, and optionally `room`, `url`, `serverIdentity` |
| `SessionAttachmentState` | `idle` \| `connecting` \| **`connected`** (carrying the connection) \| `error` — one connected state, not one per wire |

`ConnectionStatus` and `ConnectionCapability` are the host connection's, unchanged: a session speaks
the same four states and the same three capabilities, so the selector chrome, the host overlays and
the session overlay all read one vocabulary.

Capabilities are the **session's** answer, not the host's. The same host can serve a media-capable
session and a plain one.

## Reading the attach reply

`ConnectSession` / `ResumeSession` reply with `{livekitRoom, livekitUrl, livekitServerIdentity}`.
`attachmentHintFromReply(sessionId, reply)` is the one place those fields are read, and
`capabilitiesForHint(hint)` the one place the answer is turned into capabilities:

| Reply | Hint | Capabilities |
|---|---|---|
| names a room | `{sessionId, room, url?, serverIdentity?}` | `{rpc, media, presence}` |
| names none | `{sessionId}` | `{rpc}` |

Both are pure functions over plain data, testable without a rendered screen, for the same reason
`routing/selectedHost.ts` holds the selection rules.

**Blank fields are dropped, never carried as `""`.** A hint whose `room` is the empty string is a
hint a consumer can mistake for a room name — that is exactly the shape the old union forced, where
the restore path had to fabricate `{livekitUrl: "", livekitRoom: "", livekitServerIdentity: "",
identity: ""}` purely to satisfy the type. An absent field is absent, so `hint.room` is the whole
test for "is this session carried over a room".

## The two implementations

### `openLiveKitSession` — `connections/livekit/sessionConnection.ts`

The only place in the app that names a room, an identity or a token *for a session's RPC*. It owns:

- **The room join.** The `Room` object exists from construction and the join runs behind it, so
  `clientFor` answers from the first render rather than after an await. Nothing has to be awaited
  before a pane can paint; progress is reported through `status`.
- **The observer identity** — `web-traffic-<random>`, minted once per connection. Two tabs watching
  one session are two participants, and a room that saw one identity twice would drop one of them.
  The `web-` prefix is what keeps `inferParticipantRole` reading it as a browser.
- **The token and its refresh.** A token is minted for the join and re-minted a lead time before it
  lapses, under `TokenRefreshPolicy` — `leadMs`, `minDelayMs`, `retryDelayMs`, `maxRetries`. The
  floor is not a rounding detail: a daemon issuing short-lived tokens would otherwise put
  `ttl - lead` at or below zero, and since every renewal carries the same TTL the success path would
  re-arm at zero for ever — a spin loop at `setTimeout`'s 4ms clamp aimed at the daemon's own
  `TokenService`. A **refused** renewal is retried a bounded number of times and then warns once; the
  room is deliberately left up, because LiveKit may well carry on with the token it already holds and
  dropping a working terminal over a renewal nobody asked for is the worse outcome. Retrying for ever
  and giving up silently are both worse than a warning that names the session.
- **Participant targeting** — `hint.serverIdentity` when the daemon stated one, else
  `daemon-<instance>-<session>`. Deriving it is the fallback for a reply that did not say.
- **Client identity stability.** Clients are memoised per service on the connection. The room object
  a transport is built against does not change when the join lands, so a client is not re-created
  merely because the connection finished connecting; what produces a fresh client is a genuinely
  different route — another session, or a re-attach — because that is a different connection object.
  `useAcpReplay` keys an effect on the client and would cancel an in-flight snapshot pull otherwise.

The policy is an interface only so a test asserting *when* a refresh is due need not wait an hour to
see it; production states no numbers.

### `openHostServedSession` — `connections/hostServedSession.ts`

The case the attach reply names no room for: the host already serves this session's RPC on its own
roster (`cli_session_manager.rs` hosts `terminal.TerminalService` against a PTY handle). Clients,
transport and status are read straight through from the host connection — a session cannot be more
reachable than the host serving it — so identity is stable for exactly as long as the host connection
is, inherited rather than re-implemented.

**This is not a degraded path.** It is an ordinary session connection that happens to route over the
host's own transport, and it is the configuration a desktop app over IPC produces. It is not
LiveKit-shaped and not provider-shaped either — any wire whose host already carries session RPC opens
one of these, which is why it sits beside the model rather than under `livekit/`.

`close()` releases nothing, because nothing was acquired: the host connection outlives it and is
shared with every other session on it. It still latches, so a call issued after a session is detached
throws rather than quietly succeeding against a host the caller has stopped watching, and `status`
falls back to `idle` — nothing is being asked of this connection any more, which is a different claim
from the host having failed.

`openSession` refuses two things outright: a hint for a different session than the id it was given,
and a room-backed hint on a provider registered without the token client and room factory needed to
join one.

## Capabilities replace "which wire am I on"

Two statuses meant every consumer re-derived "can I do X here" from "which wire am I on".
Capabilities answer that question directly and survive a third wire:

- Which terminal component the Agent pane renders is `connection.capabilities.has("media")` — a
  session whose wire carries tracks gets the LiveKit terminal, one that carries only calls gets the
  direct stream. Never a status string.
- The handshake overlay is driven by `SessionConnection.status`, so a host-served session gets a real
  connection state. It used to be gated on `connected-livekit`, which meant the configuration that
  *works* showed the operator no connection state at all.
- The session-scoped `ConnectionService` client is one expression for every wire: the connection
  routes to the session's own process where it has one, and to the host that serves it where it does
  not — which is the daemon client the old gRPC branch reached for by hand.

`capabilitiesForHint` is the single function the media and presence gating ultimately reads through,
so the rule is stated once rather than re-derived per surface.

## Lifetime and ownership

A session connection holds real resources — a joined room and a self-rescheduling timer — so who owns
one is not a detail. The rules:

- **`SessionRuntimeRegistry` owns each runtime's connection.** A runtime outlives the focus that
  created it, so nothing else is in a position to release one. Evicting a runtime, or replacing its
  connection on re-attach, closes the connection that goes.
- **The owning screen calls `closeAll()` on unmount.** `SessionsDrawerScreen` is route-level and
  holds the registry in its own ref, so navigating away takes the last reference to every connection
  with it. Without the teardown, each session it was holding keeps a joined room and a refresh timer
  alive for the life of the page — per session, per navigation. `useCommonRoom` used to disconnect
  its room in an effect cleanup for exactly this reason; owning the connection moved that obligation
  to the registry.
- **A connection opened after its opener unmounted closes itself.** An attach is an in-flight RPC and
  the connection is opened *after* the reply lands, which can be after the screen that asked for it
  has gone. Nothing downstream ever sees such a connection, so nothing downstream can close one:
  `useSessionAttachment` releases it where it opened it.
- **`close()` is idempotent, and the in-flight join owns the release.** Closing while the join is
  still in flight must not disconnect a room that has not connected yet — that is a no-op, and the
  connect landing a moment later would leave a joined room nobody ever disconnects. So `close()`
  latches the flag, the join re-reads it when it settles, and the room is disconnected exactly once
  either way. LiveKit's `disconnect()` is not free to call twice: a second call races the reconnect
  logic of a room already on its way down.
- **An attachment never outlives the connection it names.** Releasing a runtime's connection resets
  the attachment that names the same object; otherwise every consumer — the inspector's session
  client, the traffic strip's room, the capability gates — would read `connected` off a connection
  whose `clientFor` and `transport` now throw.
- **After `close()`, `clientFor` and `transport` throw** rather than returning something that routes
  nowhere. A call issued on a detached session has no answer coming, and saying so beats leaving it
  unsettled.

The screen reads its session client from the **registry**, not from the attachment, so a session
whose runtime has been evicted yields `null` — the answer the inspector's fallback already expects —
in place of a released connection that would refuse the call.

## `status` is reachability, and must not gate on presence

`SessionConnection.status` says whether the wire is up. It deliberately does **not** wait for the
session process to appear in `remoteParticipants`.

The handshake overlay is driven off this status, and the overlay is `pointer-events-auto` over the
whole pane. A status that waited for a roster entry could leave an interactive terminal permanently
covered by an un-dismissable sheet — whenever the session process published under an identity the
connection did not predict, or left and rejoined the room. An absent peer makes a *call* fail, with
an error the caller can see and retry; a stuck overlay has no recovery at all. The two are therefore
not traded off against each other. The target identity still routes every call.

A join that genuinely failed *is* an `error` — unlike a host connection, this one can reach that
state, because minting a token and connecting a room are operations with a verdict.

A room-backed session currently has **two** handshakes in flight: this connection's, and the
terminal's own join into the same room. `SessionRuntime` shows the *less* connected of the two
(`leastConnectedOf`) and treats an error from either as an error. Reporting the connection alone
would lift the overlay over a terminal still handshaking; reporting the terminal alone is what left a
host-served session with no overlay at all.

## Reading the room back out

`liveKitRoomOf(connection)` returns the underlying `Room`, or `null` when the session is not carried
over one. It is the one thing a caller can legitimately want that the wire-neutral interface cannot
express: the traffic strip measures round-trip time off a room's own peer connection (`readRoomRtt`),
which is a measurement of the wire and not of the session.

It lives beside the join, so asking for a room still means importing LiveKit — rather than widening
`SessionConnection` with a member every other transport would answer `null` to. A **closed**
connection answers `null` too, because an attachment can outlive the connection it names by a render,
and handing it a room on its way down would have it measuring a wire nobody is on.

Before this, the traffic strip joined the session's room a second time purely to measure it. Reading
the connection's own room is the same measurement over one fewer participant.

## Watching a status from React

A `SessionConnection` publishes `status` as a value read at the moment it is asked — a room's state
changes and nothing tells React. `useConnectionStatus(connection)` therefore **samples**, on a short
interval chosen so a handshake overlay clears promptly, and returns the same object when nothing
changed so an unchanged status does not re-render every attached runtime. `null` reads as `idle`:
nothing has been asked of a connection that does not exist, which is a different claim from one that
failed.

## Current limits

- **The terminal still performs its own join.** `SessionLiveKitTerminal` mints its own
  `browser-<session>-<ts>-<random>` identity and takes its own token through
  `useLiveKitTerminalToken`, so a room-backed session holds a second participant and a second
  handshake. The identity is held as state adjusted on a changed room rather than written into a ref
  during render: a render that mutates a ref has already happened by the time React decides whether
  to keep it, so a discarded or replayed render would leave the identity and the room it was minted
  for disagreeing — and that mismatch surfaces as a token issued for the wrong room. The random
  suffix is what keeps a re-attach from colliding with a participant still leaving. Folding this join
  into the session connection is [#441](https://github.com/uppin/tddy-coder/pull/441)
  (`optional-livekit` node 5), which takes `leastConnectedOf` and the terminal's
  `onConnectionStatusChange` with it.
- **`useConnectionStatus` samples rather than subscribes.** Both wires have something that could push
  a status change — LiveKit's room `ConnectionStateChanged`, and the IPC bridge's own signal — but
  neither is reachable through the wire-neutral interface.
  [#442](https://github.com/uppin/tddy-coder/pull/442) (node 6) introduces the transport that makes a
  subscription expressible.
- **Capabilities select a terminal, but do not yet gate media.** VNC, screen sharing, participant
  video and the participant list still render regardless of what the session's connection can carry;
  [#440](https://github.com/uppin/tddy-coder/pull/440) (node 4) gates them.
- **Both terminal components still exist.** `GhosttyTerminalLiveKit` and `GhosttyTerminalGrpc` are
  chosen between, not merged; the merge is node 5's.
