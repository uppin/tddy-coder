# 2026-09-06 — Memory, load average and core count in host telemetry

The Host Stats Footer and every Hosts row now report a host's **available memory** alongside its
existing disk and CPU readings, and the footer adds the host's **load average**. An operator can see
that a machine is out of memory — the more common reason a session dies than a busy CPU — without
opening a terminal on it.

A host whose platform reports no load average renders `—`, never `0.00`. The two are different
answers and the UI keeps them distinguishable: an unsupported platform must not read as an idle
machine.

The event also carries an explicit logical core count, so a reader is not left inferring it from a
per-core array that is empty before the first sample.

See [`host-stats-footer.md`](../host-stats-footer.md) and
[`hosts-screen-telemetry.md`](../hosts-screen-telemetry.md).
