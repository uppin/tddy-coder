# 2026-09-06 — Live CPU and disk on every Hosts row

The Hosts screen shows each host's per-core CPU and free disk as a live reading, streamed from that
host rather than polled. Comparing load across the fleet no longer means selecting each machine in
turn and reading the footer.

Rows that cannot report say so rather than showing zeroes: an offline host, a host nothing routes to,
and a host that has connected but not yet reported are three visibly different states. Subscriptions
open only for hosts that can actually be read, one apiece, and are cancelled when the screen is left.

The Host Stats Footer is unchanged.

See [`../hosts-screen-telemetry.md`](../hosts-screen-telemetry.md).
