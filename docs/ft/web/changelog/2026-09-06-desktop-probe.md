# 2026-09-06 — Whether a host has a desktop to reach, and whether tddy could bridge it

Every Hosts row now reports, per protocol, whether a **remote desktop is reachable** on that host —
VNC on `:5900`, RDP on `:3389` — and, separately, whether that host's daemon can **bridge** one at
all. tddy has bridged both protocols for a while; what it never said was whether there was anything
to bridge, or whether the bridge binary was even installed. That second fact used to announce itself
as a spawn error, after the operator had already asked for a stream.

**The two facts are shown side by side and never merged.** "tddy cannot bridge here" and "nothing is
serving a desktop here" are unrelated problems: one wants a binary installed, the other wants a
desktop server started. A single "unavailable" verdict would send an operator to the wrong one, so
the row reads `Bridge ready` / `No bridge` beside `Desktop on :5900` / `No desktop on :5900`.

**"We checked and the answer is no" is not "we could not check."** A refused connection is a
finding — the host was reached and nothing was listening. A timeout, an unreachable network or a
denied connect is not, and reads `Could not check :5900` with the reason on hover. A row keying off
reachability alone would report "No desktop" for a host it never touched, which is the same
fabricated fact this screen's git and `gh` cells already refuse to state.

**The port that was checked is always named.** Only the default ports are probed — port discovery is
out of scope — and a host is free to serve on another display, so an unqualified "No desktop" would
read as authoritative about the host rather than about `:5900`.

**The probe is a bare TCP connect, closed immediately, and never a protocol handshake.** A
monitoring screen pointing half-open RFB handshakes at people's desktops on a timer is antisocial,
and the connect already answers the question. Each connect is bounded at 750 ms so an unreachable
address cannot hold the row's other cells up.

Nothing here starts a stream, spawns a bridge or opens a viewer, and the per-session screen-sharing
flow is untouched. Opening a host's desktop is
[PR #460](https://github.com/uppin/tddy-coder/pull/460).

Like the rest of the tooling section, this is not yet reachable in the running app: no screen mounts
`HostRowTooling` and nothing issues `GetHostTooling` — see
[`hosts-screen-tooling.md`](../hosts-screen-tooling.md) § *Not yet reachable in the running app*.

Node 7 of the `#hosts-screen` stack — [PR #459](https://github.com/uppin/tddy-coder/pull/459).

See [`hosts-screen-tooling.md`](../hosts-screen-tooling.md) and
[`screen-sharing-sessions.md`](../screen-sharing-sessions.md).
