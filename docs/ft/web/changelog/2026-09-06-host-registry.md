# 2026-09-06 — A Hosts screen that remembers the machines you are not talking to

Everywhere else in tddy, the list of hosts is the list of hosts that are answering. A machine that
goes offline simply disappears — the selector stops offering it, and nothing anywhere says it ever
existed. That is right for choosing where to run a session, and it removes the row at exactly the
moment you want it: the host is unreachable, and you want to know what it was and when it was last
alive.

**`#/hosts` is that list.** One row per host tddy knows about — reachable or not — showing its label,
whether it is online now, when it was last seen, its instance id and where it keeps its checkouts.
The daemon serving the page is marked `(local)`, and rows sort online-first then alphabetically, so
the hosts you can act on are at the top and the rest stay listed rather than vanishing. Reach it from
the hamburger menu, between Projects and Models & Agents.

The list lives in the daemon, not the browser. It survives a daemon restart and is the same list in
every browser and on every device — a host you used yesterday from a laptop is still there when you
open the dashboard from a phone. Nothing is ever removed: going offline records the time and nothing
else, and a machine's first-seen time is never re-stamped, so "known since" keeps its meaning.
Being online, by contrast, is worked out fresh each time the screen is opened — a remembered "online"
flag would be wrong the moment a daemon exited without warning.

One machine shows as one row, however many times its daemon restarts, and a host that is answering
right now is always listed whether or not it has been recorded before. A host that stays up for weeks
still reports a recent last-seen time, so it never reads as "last seen a month ago" the moment it
does go away.

Nothing about choosing or reaching a host changes: the host selector, the host directory and per-host
connections work exactly as before, and this screen opens no connection of its own.

The root of the `#hosts-screen` stack, in [#453](https://github.com/uppin/tddy-coder/pull/453). The
columns that hang off these rows — live per-host telemetry, host resources, git and `gh` identity,
agent keys, remote-desktop availability and connect — follow in
[#454](https://github.com/uppin/tddy-coder/pull/454) through
[#460](https://github.com/uppin/tddy-coder/pull/460).

Feature [hosts-screen.md](../hosts-screen.md), [app-shell.md](../app-shell.md),
[url-state-routing.md](../url-state-routing.md); technical
[hosts-screen.md](../../../../packages/tddy-web/docs/hosts-screen.md),
[host-registry.md](../../../../packages/tddy-daemon/docs/host-registry.md).
