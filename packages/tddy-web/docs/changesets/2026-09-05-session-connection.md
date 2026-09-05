# 2026-09-05 — a session attachment is one connection, whatever wire carries it

**Type:** Architecture

Attaching to a session was where LiveKit sat deepest in `tddy-web`, and where the codebase already
admitted a second path without abstracting it. `useSessionAttachment` branched on
`resp.livekitRoom !== ""` and produced one of two statuses: `connected-livekit`, which joined a
**second** LiveKit room per session (`useSessionLiveKitRoom`, minting a `web-traffic-*` observer
identity), targeted the session process's own participant (`sessionParticipantRpcClient`), minted and
refreshed a browser token (`useLiveKitTerminalToken`) and cached clients against the `Room` object
(`sessionClientCache`); and `connected-grpc`, on which session RPC quietly fell back to the *daemon*
client and the connection handshake overlay never appeared at all, because it was gated on the other
status. Both were threaded by hand through `SessionRuntime`, `sessionRuntimeRegistry`,
`SessionMainPane`, `SessionDetailPane` and `SessionsDrawerScreen` — so every consumer knew which wire
it was on, and a third wire meant a third branch in every one.

`HostConnection.openSession(sessionId, hint)` now returns a `SessionConnection` — `status`, `error`,
`capabilities`, `clientFor`, `transport`, `close` — and `SessionAttachmentState` has **one** connected
state carrying it. `SessionAttachmentHint` is the attach reply read into transport-neutral terms by
`attachmentHintFromReply`, the only place those proto fields are read; blank-but-present fields are
dropped rather than carried as `""`, so `hint.room` is the whole test for "is this session carried
over a room" and nothing has to fabricate four empty LiveKit fields to satisfy a union.
`capabilitiesForHint` states the rule once: a room-backed session is `{rpc, media, presence}`, a
host-served one `{rpc}`.

`openLiveKitSession` (`connections/livekit/sessionConnection.ts`) owns the room join, the observer
identity, the token and its TTL refresh, and `daemon-<instance>-<session>` targeting — the only place
in the app that names any of those for a session's RPC. The room object exists from construction and
the join runs behind it, so `clientFor` answers from the first render and a client's identity is as
stable as its routing, which is the guarantee `SessionClientCache` used to give. `TokenRefreshPolicy`
floors the refresh delay (a short TTL otherwise re-armed at zero for ever, spinning `refreshToken` at
the timer's 4ms clamp) and bounds retries on a refused renewal, warning once when spent rather than
letting the room drop silently at expiry.

`openHostServedSession` routes over the host connection itself, inheriting its clients, transport and
status. It is not a degraded path — it is what `cli_session_manager.rs` already serves and what a
desktop app over IPC will produce — so it now gets a real handshake overlay instead of none.

Capabilities replace "which wire am I on": the terminal component is chosen by
`capabilities.has("media")`, and the overlay by `SessionConnection.status`. That status is
**reachability only** and deliberately does not wait for the session process to appear in
`remoteParticipants` — the overlay is `pointer-events-auto` over the whole pane, so a status that can
hang leaves an un-dismissable sheet over an interactive terminal, while an absent peer merely fails a
call the caller can see and retry. A room-backed session still has a second handshake (the terminal's
own join), so the overlay shows the less connected of the two via `leastConnectedOf`.

Lifetime is now owned rather than incidental. The registry owns each runtime's connection and closes
it on eviction, on re-attach and on `closeAll()` from the screen's unmount; a connection whose opener
unmounted before the reply landed closes itself; `close()` is idempotent and the in-flight join owns
the room's single release; and an attachment is reset when the connection it names is released, so
nothing reports `connected` for a connection whose `clientFor` now throws. `liveKitRoomOf` is the one
escape hatch — the traffic strip measures RTT off the session's own room instead of joining it a
second time.

Not here: the terminal's own join and `useLiveKitTerminalToken`
([#441](https://github.com/uppin/tddy-coder/pull/441)), a status subscription in place of
`useConnectionStatus`'s sampling ([#442](https://github.com/uppin/tddy-coder/pull/442)), and gating
the media and presence surfaces on capability
([#440](https://github.com/uppin/tddy-coder/pull/440)). Both terminal components still exist; only
the choice between them changed.

No proto change, no daemon change, no new npm dependency. Tests: 1034 unit, `vite build` clean, 71/71
Cypress over the reworked and at-risk specs. Technical
[session-connections.md](../session-connections.md), feature
[session-drawer.md](../../../../docs/ft/web/session-drawer.md). PR
[#439](https://github.com/uppin/tddy-coder/pull/439).
