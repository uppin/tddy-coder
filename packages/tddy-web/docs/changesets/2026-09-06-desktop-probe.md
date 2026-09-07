# 2026-09-06 — The remote-desktop section on a Hosts row
**Type:** Feature

`HostRowRemoteDesktop` renders the desktop half of one `GetHostTooling` answer, taking
`{instanceId, readings}` and fetching nothing itself. `HostRowTooling` mounts it behind an optional
prop, following the precedent the ssh-agent section set — a row renders before any probe has
answered, and an absent block is an empty list rather than a fabricated reading.

Each reading is one `hosts-row-<id>-<vnc|rdp>` span carrying **two facts side by side**: the bridge
half (`Bridge ready` / `No bridge`) and the desktop half (`Desktop on :5900` / `No desktop on :5900`
/ `Could not check :5900`). They are never merged into one verdict, because a host can serve a
desktop tddy has no bridge for and a host with the bridge installed can be serving nothing — and the
two "no"s ask for work on different things.

The desktop half repeats the **outcome-first** guard the git, `gh` and ssh-agent sections use rather
than sharing one: anything other than `ProbeOutcome.OK` reads "Could not check", with
`failureReason` on the `title`. Keying off `desktopReachable` alone would render "No desktop" for a
host the daemon never reached — the one thing this section exists not to say.

The checked port is part of every desktop string rather than a cell of its own. Only default ports
are probed, so an unqualified "No desktop" would read as authoritative about a host that simply
serves on another display.

`PROTOCOL_NAMES` maps `screen_sharing.proto`'s `Protocol` values (`1` → VNC, `2` → RDP), the same
values the daemon puts on the wire. A reading for any other value renders **nothing**: `protocol` is
an open proto3 value, a newer daemon can probe a protocol this bundle cannot name, and a nameless row
of facts is worse than an omitted one.

`HostsScreenRemoteDesktopAcceptance.cy.tsx` covers four behaviours, 4/4 green, and two of them assert
a **denial** as well as a presence — `No desktop` must not also read `No bridge`, and
`Could not check` must not read `No desktop`. A spec that only asserts its own string passes for a
component that collapses the two facts, which is the exact bug the section exists to prevent.
`hostRemoteDesktopPage` on the shared page object exposes `section(id)` and
`protocol(id, "vnc" | "rdp")`, so a test names a protocol rather than a test id.

⚠ Still **not reachable in the running app**: nothing in `src/` mounts `HostRowTooling` or issues
`GetHostTooling`, so this section's only call site is its own spec — the same standing gap the git,
`gh` and ssh-agent sections have, and it closes in the same move.

Node 7 of the `#hosts-screen` stack — [PR #459](https://github.com/uppin/tddy-coder/pull/459).

See [`hosts-screen.md`](../hosts-screen.md).
