# 2026-09-06 — Host telemetry reports memory, load average and core count
**Type:** Feature

Spans `tddy-service` (proto), `tddy-daemon` (provider + handler) and `tddy-web` (hook + indicators).

`HostStatsEvent` gains a memory block, an optional load block and an explicit logical core count.
The daemon reads them from the same long-lived `sysinfo::System` that already backs CPU, and carries
memory and load on the existing fast tick rather than adding a third timer.

The load average is the design-bearing part. `sysinfo` returns all zeros on platforms that have none,
and `0.00` in a UI reads as "idle" — the opposite of "cannot tell". So the absence is modelled
end to end: `Option<LoadAverage>` in the trait, an omitted block on the wire, `null` on the hook, and
an em dash in the UI. Nothing along that path substitutes a zero.

**Deferred at wrap.** The Hosts row memory cell ships without a test, and `logicalCores` reaches the
hook's result type with no consumer. Both are recorded under *Missing coverage* in
[`docs/dev/TODO.md`](../TODO.md) rather than tracked in a working document.

Node 3 of the `#hosts-screen` stack — [PR #455](https://github.com/uppin/tddy-coder/pull/455).

See [`packages/tddy-daemon/docs/connection-service.md`](../../../packages/tddy-daemon/docs/connection-service.md)
and [`packages/tddy-web/docs/host-stats-streaming.md`](../../../packages/tddy-web/docs/host-stats-streaming.md).
