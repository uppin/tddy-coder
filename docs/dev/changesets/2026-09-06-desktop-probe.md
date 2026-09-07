# 2026-09-06 — Whether a host has a desktop to reach, and a bridge to reach it with
**Type:** Feature

Spans `tddy-service` (the `HostRemoteDesktop` message on `GetHostToolingResponse`), `tddy-daemon`
(`remote_desktop_probe.rs`, the block in `host_tooling.rs`, the wire conversion in
`connection_service.rs`) and `tddy-web` (`HostRowRemoteDesktop`).

The fourth block on node 4's tooling probe rather than a fourth near-identical RPC — the same
consumed-and-extended relationship the ssh-agent block has to it, and the reason that RPC was built
to grow blocks.

**The design-bearing rule is that one word can hide two facts.** "This host has a desktop available"
is really two claims: can this host's daemon spawn a bridge at all, and is anything serving a desktop
here. They are independent, and their "no"s ask for unrelated work — install a binary, versus start a
desktop server. Both travel on the wire and both are rendered, side by side, because a merged verdict
would send an operator to the wrong machine's worth of work.

Its second edge is the one node 4 already drew for `gh`: **"we checked and the answer is no" is not
"we could not check."** A refused connection is a finding; a timeout, an unreachable network or a
denied connect is not, and arrives as `ProbeOutcome::Failed` beside a neutral `desktop_reachable:
false`. Four states per protocol, kept distinguishable end to end. The checked port travels with
every reading too, since only the default ports (5900, 3389) are probed and a host serving on another
display would otherwise be reported as having no desktop at all.

The probe is deliberately **the least it can be**: a TCP connect that writes no bytes and closes,
bounded at 750 ms, plus a `stat` on the resolved bridge path. Never a protocol handshake — a screen
that polls every host it lists must not be pointing half-open RFB handshakes at people's desktops —
and never a stream: `ScreenSharingService`, its per-session targets, its vault and its path
resolution order are all untouched.

The `exists()` check is the cheapest real improvement in the node, and its subtlety is where the bugs
were. It must test the path **this** daemon would spawn, so the probe carries the daemon's own
`DaemonConfig` rather than a default one; and a bare `PATH` name is searched along `PATH` rather than
`exists()`-ed, since the latter answers about the daemon's working directory and would report a
genuinely installed bridge as absent. Both mistakes fabricate the same fact the node exists to
prevent, in opposite directions.

**Deferred, recorded here rather than in a working document:** probing on every screen refresh costs
a TCP connect per host per protocol, with no cache or in-flight dedup ahead of a UI that polls — the
same standing gap the other three blocks have. Non-default ports are not discovered. And no code path
issues `GetHostTooling` yet: `HostRowTooling` is unmounted, so this section, like its three
neighbours, is reachable over the wire and invisible in the app.

Node 7 of the `#hosts-screen` stack — [PR #459](https://github.com/uppin/tddy-coder/pull/459).
Opening a host's desktop in the in-app viewer is
[PR #460](https://github.com/uppin/tddy-coder/pull/460).

See [`packages/tddy-daemon/docs/host-tooling-probe.md`](../../../packages/tddy-daemon/docs/host-tooling-probe.md),
[`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md),
[`docs/ft/web/hosts-screen-tooling.md`](../../ft/web/hosts-screen-tooling.md) and
[`docs/ft/web/screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md).
