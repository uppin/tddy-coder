# Daemon selector + LiveKit-only RPC routing

## Purpose

The shadcn-based tddy-web screens (`ProjectsAppPage`, `WorktreesAppPage`, `VmsAppPage`,
`TasksDrawerScreen`, `RpcPlaygroundAppPage`, `ConnectionScreen`, `SessionsDrawerScreen`) each talk
to a single **serving daemon** — the `tddy-daemon` instance that served the web bundle. An operator
with several daemons in the same LiveKit common room (e.g. a laptop and a workstation both running
`tddy-daemon`) has no way to point the UI at a different daemon without reloading the page against
that daemon's own URL.

This feature adds a **daemon selector** to the top-right strip of these screens. The selectable
daemons are the **daemon-role LiveKit participants in the common room** — the same source already
used by the Projects screen's host picker (`daemonHostsFromParticipants`). Selecting a daemon
switches **all daemon-level RPC** (projects, worktrees, VMs, tasks, session list/start) to that
daemon, without a page reload.

## Why LiveKit-only for daemon-level RPC

HTTP `/rpc` is served same-origin by the daemon that served the web bundle. Pointing an HTTP
ConnectRPC client at a *different* daemon's origin is cross-origin and blocked by CORS (the daemons
do not — and should not — run a permissive CORS policy for their `/rpc` endpoint). LiveKit RPC
(ConnectRPC over LiveKit data channels, `tddy-livekit-web`'s `LiveKitTransport`) has no such
restriction: any daemon reachable in the common room can be addressed over the LiveKit data
channel it already publishes/subscribes on. The daemon already serves the full daemon-level
service set over both bindings from the same `rpc_entries` (see
[`tddy-daemon` RPC dispatch](../../../packages/tddy-daemon/docs/connection-service.md)), so LiveKit
RPC is a drop-in substitute — **except** for the initial bootstrap, which must stay HTTP to the
serving daemon:

- `GET /api/config` — how the web learns the LiveKit URL, common room name, and (new) the serving
  daemon's own instance id.
- `TokenService.generateToken` / `refreshToken` — how the web obtains the LiveKit token used to
  join the common room in the first place. There is no LiveKit connection to route this token
  request over yet.

Everything else — `ConnectionService`, `TaskService`, `ActionService`, `VmService`,
`ScreenSharingService`, `AuthService` — switches to LiveKit RPC, addressed at the **selected**
daemon.

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
