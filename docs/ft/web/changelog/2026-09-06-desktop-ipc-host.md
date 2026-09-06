# 2026-09-06 — The desktop app talks to its own daemon, with or without LiveKit

The Tauri desktop app runs `tddy-daemon` inside its own process, and until now the dashboard could
not talk to it as a **host**. Reaching a daemon meant sharing a LiveKit common room with it, and the
desktop app does not join one unless you configure it — so the host selector offered the machine you
were sitting at and every screen then reported no connection to it.

It is now reached over the app's own in-process bridge. **With no LiveKit configuration at all the
desktop app is fully functional on its own host**: no room is joined, no token is minted, no `Room`
object is constructed, and the media and presence surfaces are absent rather than broken — a VNC tab
that never paints is worse than no VNC tab.

**With LiveKit configured, both work at once and neither is a mode.** Peers in the common room are
reached over LiveKit with video, screen sharing and a participant roster; the machine you are sitting
at stays on the in-process bridge, in the same session of the app, with no reload and no setting to
choose between them. It stays there deliberately even when the common room could also reach it: the
daemon is in the same binary, so going out to a media server and back is latency for nothing.

The two are independent, which is the point. **If the common room fails, you lose the peers and
nothing else** — your own host stays selectable and fully usable, the host list keeps working, and
the failure is reported against the common room rather than as a fault of the app.

The trade is stated rather than hidden: a host reached over the in-process bridge carries **RPC
only**. Video, screen sharing and the participant list belong to LiveKit, and the same machine is
media-capable when a browser reaches it over the common room and not when its own desktop app reaches
it in-process. Publishing media into a room to fill that gap would have made the desktop app quietly
require the thing this work made optional.

Nothing about the browser changes. The desktop machine's host is still reached over LiveKit from a
browser, exactly as before — one bundle serves both, and the in-process wire is simply never
registered on a page a browser loaded.

The last node of the `optional-livekit` stack, in
[#443](https://github.com/uppin/tddy-coder/pull/443), on the addressed IPC connections from
[#442](https://github.com/uppin/tddy-coder/pull/442) and the capability gating from
[#440](https://github.com/uppin/tddy-coder/pull/440).

Feature [tddy-desktop-tauri.md](../../desktop/tddy-desktop-tauri.md),
[daemon-selector-livekit-rpc.md](../daemon-selector-livekit-rpc.md); technical
[local-host-ipc.md](../../../../packages/tddy-web/docs/local-host-ipc.md).
