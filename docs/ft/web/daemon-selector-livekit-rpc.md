# Daemon selector + host-connection RPC routing

## Purpose

The shadcn-based tddy-web screens (`ProjectsAppPage`, `WorktreesAppPage`, `VmsAppPage`,
`TasksDrawerScreen`, `RpcPlaygroundAppPage`, `ConnectionScreen`, `SessionsDrawerScreen`) each talk
to a single **serving daemon** — the `tddy-daemon` instance that served the web bundle. An operator
with several daemons in the same LiveKit common room (e.g. a laptop and a workstation both running
`tddy-daemon`) has no way to point the UI at a different daemon without reloading the page against
that daemon's own URL.

This feature adds a **daemon selector** to the top-right strip of these screens. The selectable
daemons come from the **host directory** — the merge of every registered directory source. The
common room is one source (daemon-role participants, via `daemonHostsFromParticipants`); the daemon
that served the page is another, so a build that joins no common room still has a host to offer.
Selecting a daemon switches **all daemon-level RPC** (projects, worktrees, VMs, tasks, session
list/start) to that daemon, without a page reload. See
[`tddy-web` host directory](../../../packages/tddy-web/docs/host-directory.md).

## Why daemon-level RPC does not use HTTP

HTTP `/rpc` is served same-origin by the daemon that served the web bundle. Pointing an HTTP
ConnectRPC client at a *different* daemon's origin is cross-origin and blocked by CORS (the daemons
do not — and should not — run a permissive CORS policy for their `/rpc` endpoint). So reaching a
peer daemon needs a wire that is not the browser's origin model.

A **host connection** is the name for that wire, and it is deliberately not spelled in any one
transport's vocabulary. A call site asks for a connection to a host id; a registered
`ConnectionProvider` supplies it. LiveKit is the provider today: ConnectRPC over LiveKit data
channels (`tddy-livekit-web`'s `LiveKitTransport`) can address any daemon in the common room over
the data channel it already publishes and subscribes on. A build that reaches a host some other way
registers its own provider, and no screen learns which wire it got. The model, the registry and its
hooks are described in
[`tddy-web` host connections](../../../packages/tddy-web/docs/host-connections.md).

The daemon serves the full daemon-level service set over both bindings from the same `rpc_entries`
(see [`tddy-daemon` RPC dispatch](../../../packages/tddy-daemon/docs/connection-service.md)), so a
peer connection is a drop-in substitute for the HTTP client — **except** for the initial bootstrap,
which must stay HTTP to the serving daemon:

- `GET /api/config` — how the web learns the LiveKit URL, common room name, and (new) the serving
  daemon's own instance id.
- `TokenService.generateToken` / `refreshToken` — how the web obtains the LiveKit token used to
  join the common room in the first place. There is no LiveKit connection to route this token
  request over yet.

Everything else — `ConnectionService`, `TaskService`, `ActionService`, `VmService`,
`ScreenSharingService`, `AuthService` — resolves through a host connection, addressed at the
**selected** daemon.

Who the hosts *are* is the **host directory**'s answer, not the common room's. An unconfigured
common room contributes no hosts and reports `idle` rather than `error` — an operator who chose not
to configure LiveKit is not shown a connection failure for it — and the daemon serving the page is
contributed regardless, so the selector is never empty on a daemon-served page.

Naming a host is not the same as reaching one: until a wire is registered that can reach the serving
daemon, selecting it resolves no connection and each screen renders its own "no connection" state.
That wire arrives later in the `optional-livekit` stack.

## Scope boundary: daemon-level vs. per-session RPC

**Per-session** communication is unaffected by daemon selection and keeps targeting its own
session's server identity in its own LiveKit room, exactly as today:

- The terminal (`terminal.TerminalService`, `GhosttyTerminalLiveKit`) — targets
  `daemon-{instanceId}-{sessionId}`.
- The PR-Stack Chat Screen presenter stream (`usePresenterLiveKitRoom`) — targets the session's
  presenter identity.
- Session inspector streams (VNC, screen sharing).

Only **daemon-level** RPC — calls that are not scoped to one already-attached session — switch
with the selector.

**Exception — cross-host session aggregation.** The sessions drawer deliberately does *not* scope
`ListSessions` to the selected daemon. It fans the call out to **every** advertised daemon (one
client per host, resolved through that host's connection) and merges the results, so a session with
a live participant on a non-selected host stays visible (see
[session-drawer.md § Cross-Host Active Sessions](./session-drawer.md#cross-host-active-sessions)).
Interaction with such a row routes attach/resume/delete/terminate to that session's **owning**
daemon via `useDaemonClientFor` — without calling `selectDaemon`, so the selected host is unchanged.

## What a host connection cannot substitute for

Daemon-level RPC is wire-neutral, and the connection model is what makes it so. **Tracks and
presence are not.** A video frame and a participant roster are things a wire either carries or does
not, so the surfaces built on them are gated on the selected host's connection rather than
abstracted: they render where that connection advertises the capability, and where it does not they
are removed from navigation and name the reason.

| Surface | Needs | Where it is documented |
|---|---|---|
| Session inspector's VNC and Screen Sharing tabs | video tracks | [vnc-sessions.md](vnc-sessions.md), [screen-sharing-sessions.md](screen-sharing-sessions.md) |
| Participant roster and camera preview | presence (and tracks, for the camera column) | [app-shell.md § LiveKit screen](app-shell.md#livekit-screen) |
| `#/livekit` and its nav entry, and the rooms panel | presence | [app-shell.md](app-shell.md), [livekit-rooms-panel.md](livekit-rooms-panel.md) |
| RPC Playground's participant picker | presence | below |
| Cross-host session rows in the sessions drawer | presence | [session-drawer.md § Cross-Host Active Sessions](session-drawer.md#cross-host-active-sessions) |

**The RPC Playground's participant picker** is a presence surface, not an RPC one: its options *are*
common-room participants. On a host reached over a wire that carries none, the picker and its label
are replaced together by the reason there is nobody to address — a `<label for>` pointing at a
control that is not there is a promise to a screen reader that nothing keeps. The rest of the
playground, which addresses the selected host over its connection, is unaffected.

A join that is still in flight, or one that failed, is **not** an absent capability: those surfaces
stay, because a page that withdrew them for the second or two every LiveKit page spends connecting
would contradict itself, and because a failed join's reason is what an operator opens those screens
to read. The shared rule is
[capability gating](../../../packages/tddy-web/docs/capability-gating.md).

## The daemon identity subtlety

A `tddy-daemon` joins the common room as **two** LiveKit participants:

- A **discovery** participant, identity = the bare instance id (e.g. `udoo`). It publishes the
  daemon advertisement metadata (`{"instance_id":"udoo","label":"udoo (this daemon)"}`) that
  `daemonHostsFromParticipants` reads to build the selectable list.
- An **RPC-server** participant, identity = **`daemon-{instanceId}`** (e.g. `daemon-udoo`). This is
  the participant that actually serves `ConnectionService`/`TaskService`/etc.

The selector lists daemons by their discovery identity (the human-recognizable instance id), but
daemon-level RPC must address **`daemon-{instanceId}`**. This mapping is a fixed `daemon-` prefix,
not a lookup.

A related subtlety: every daemon's own advertisement self-labels `"{id} (this daemon)"` from *its
own* perspective — so this substring is not a signal of which daemon is serving *this particular
web session*. To default the selector to the serving daemon and to display "(this daemon)" only
next to the correct entry, the web needs the serving daemon's own instance id, which `/api/config`
now exposes as `daemon_instance_id`.

## Screen changes

- A `DaemonSelector` (shadcn `Select`) renders in the top-right of each daemon-mode screen's header
  strip, next to `UserAvatar` (or the equivalent top strip for drawer screens).
- It lists the common-room daemon-role participants, labels stripped of the self-referential
  `(this daemon)` suffix, with that suffix re-added **only** to the entry matching the serving
  instance id.
- Selecting an entry re-targets all daemon-level RPC clients used by the current screen at
  `daemon-{selectedInstanceId}`. The selection persists for the browser tab (`sessionStorage`) and
  defaults to the serving daemon.
- The **Projects** screen host picker ("Add to host") is migrated to source its daemon list from
  the same shared common-room context as the selector, instead of opening its own separate common-
  room connection (today `ProjectsAppPage` calls `useCommonRoom` independently). Add-to-host also
  addresses the chosen target host **directly** (a client for `daemon-{targetInstanceId}` via
  `useDaemonClientFor` / the transport factory) rather than double-hopping through the selected
  daemon.
- **Session creation** (`CreateSessionPane`, used by the sessions drawer and the PR-stack
  `CreateSessionDialog`) gains a **Host** `<select>` sourced from the same shared daemon list. It
  is shown only when the common room advertises at least one daemon (so single-daemon / no-provider
  usage is unchanged), defaults to the selected daemon, and threads the chosen `daemonInstanceId`
  into `StartSession` and `ListProjectBranches`.

## Trust model

Unchanged from the existing multi-host trust model (see
[Projects screen & multi-host projects § Trust model](projects-screen-multi-host.md#trust-model)):
any participant able to join the common room is treated as an eligible daemon; the common room is
a trusted peer group, not a cryptographically authenticated one. Routing daemon-level RPC to a
peer over LiveKit does not change what that peer could already do via `AddProjectToHost`/
`StartSession` forwarding — the RPC now simply reaches it directly instead of via daemon-to-daemon
forwarding.

## Related documentation

- **[Projects screen & multi-host projects](projects-screen-multi-host.md)** — the existing daemon
  host picker this reuses as the shared data source.
- **[LiveKit common room: owned project count](livekit-participant-owned-projects.md)** —
  participant discovery + role inference this selector is built on.
- **[Web terminal / common room](web-terminal.md#shared-livekit-room-livekitcommon_room)** — the
  shared LiveKit room; per-session rooms are unaffected by this feature.
- **[Capability gating](../../../packages/tddy-web/docs/capability-gating.md)** — the one predicate
  and the availability rule behind every surface in the table above.
