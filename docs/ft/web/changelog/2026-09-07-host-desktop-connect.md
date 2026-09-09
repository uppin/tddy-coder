# 2026-09-07 — Open a host's desktop from the Hosts screen

A remote desktop can belong to a **machine** instead of to a coding session. Where a Hosts row
reports a reachable desktop and a daemon that can bridge it, that row now offers a **connect**
action, and it opens the same full-screen overlay the session inspector uses — no session created
first, and nothing new on the rendering side.

**Host scope is an addressing and storage change, and nothing more.** The same `tddy-vnc` /
`tddy-rdp` bridges, the same JSON-on-stdin configuration so no credential reaches a command line,
the same LiveKit video track, the same overlay. A host's desktops are kept in
`host-desktop-targets.json` in the daemon's host-registry directory, next to the other per-machine
state and deliberately **not** inside the per-session encrypted vault: a desktop outlives the work
done on it, and deleting a session must not delete a machine's desktop. Neither store can see the
other's targets.

**The desktop password is prompted and kept nowhere.** The daemon raises the question on the host's
encrypted prompt channel, stamped with the GitHub user whose call is waiting on it, so it reaches
that operator's browser and no other. The browser encrypts the answer under the key the host
published with the question; the daemon decrypts it, hands it to the bridge on the bridge's stdin
and drops it. It is never written to disk, never held past the call, and never appears in a process
argument. That is deliberately unlike the per-session vault, which stores its credential — the Hosts
screen prompts and drops for every secret it handles, and one model there beats two. The daemon asks
on **every** open, because nothing records whether a desktop wants a password; an empty answer is a
real answer and is how a password-less desktop is opened, and a question nobody answers fails the
start rather than spawning a bridge that could authenticate to nothing.

**Without the `media` capability the action is absent, not disabled.** A frame pipe carries no
video, so on a host reached that way a LiveKit track cannot arrive however reachable its desktop is,
and a control that provably cannot work is worse than no control. The reachability facts beside it
are still reported.

**Rendering is always a daemon-produced video track.** There is no browser-side VNC or RDP protocol
client and there must never be one — a permanent architectural boundary, not a stage of the work.

⚠ **A connected desktop is view-only**, on this path and on the per-session one. The daemon and the
bridge implement input forwarding in full — the bridge serves `ScreenSharingInputService` over its
LiveKit data channel and injects every event it receives — but **no browser client opens that
stream**, so nothing sends any. The remaining work is one client in the overlay, and it fixes both
paths at once:
[`docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md`](../../../dev/todo/2026-09-07-remote-desktop-input-forwarding.md).

Every host-scoped call carries the operator's session token, and a call without one is rejected.

Top node of the `#hosts-screen` stack — [PR #460](https://github.com/uppin/tddy-coder/pull/460).

See [`screen-sharing-sessions.md`](../screen-sharing-sessions.md) and
[`hosts-screen-tooling.md`](../hosts-screen-tooling.md).
