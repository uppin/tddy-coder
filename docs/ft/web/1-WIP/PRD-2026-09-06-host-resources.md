# PRD — Host telemetry: memory, load average and core count

**Date:** 2026-09-06
**PRD type:** Modification to an existing feature
**Product area:** `web` (surface) + `daemon` (provider)
**Stack:** `#hosts-screen` node 3 of 8
**Branch:** `feature/hosts-screen/host-resources` → base `feature/hosts-screen/telemetry-fanout`

## Affected features

| Document | Relationship |
|---|---|
| [`docs/ft/web/host-stats-footer.md`](../host-stats-footer.md) | Defines the telemetry contract this node extends; its footer gains the new readings too |
| [`docs/ft/web/hosts-screen-telemetry.md`](../hosts-screen-telemetry.md) | Supplies the per-host fan-out that carries the new fields to every row (`#hosts-screen` 2/8) |

## Summary

Complete the host telemetry surface: add **memory** (total / available), **load average**, and an
**explicit logical core count** to `HostStatsEvent`, so every Hosts row — and the existing sessions
footer — reports a host's actual headroom rather than only CPU and one disk.

## Background

`sysinfo` is already the daemon's dependency and already long-lived for correct per-core deltas, but it
is never asked for memory or load. Grep across the workspace for `total_memory`, `available_memory`,
`refresh_memory`, `load_average`, `/proc/loadavg`, `/proc/meminfo` returns **zero product hits**. Core
count exists only implicitly, as `per_core_percent.len()`.

The result is that an operator can see a host's CPU is busy but not whether it is out of memory — which
is the more common reason a session fails.

## Proposed changes

### What is changing

- **`HostStats` trait** gains readings for memory and load average; core count comes from the existing
  CPU snapshot but is reported explicitly on the wire.
- **`SysinfoHostStats`** implements them via `refresh_memory()` / `total_memory()` /
  `available_memory()` and `sysinfo`'s load-average accessor.
- **`HostStatsEvent`** gains a memory block and a load block, carried on the **fast (CPU) tick**, since
  both change on that timescale.
- **The Hosts row** shows memory alongside CPU and disk; **the sessions footer** gains the same
  readings, since it consumes the same event.

### Load average is optional, and says so

On platforms where a load average is not available, `sysinfo` reports zeros. **Reporting `0.00` as a
real reading would be a fabricated value** — an operator reads that as an idle machine. The wire
therefore distinguishes "not reported" from "reported as zero", and the UI renders the former as `—`.

### What is staying the same

- **The cadence model is unchanged**: two timers, `cpu_tick` and `disk_tick`, each pushing a full
  event. No third interval is introduced.
- **The always-populated contract holds** — an event continues to carry the latest snapshot of every
  block.
- The per-host fan-out, the registry, the route and the screen shell are earlier nodes' and are not
  modified.

## Impact analysis

### Technical

- **Adding a trait method breaks every implementor.** All implementors are in-repo: `SysinfoHostStats`
  plus the two test doubles in `connection_service.rs`. `SequencedHostStats` advances a counter per
  read to prove cadence — the new readings must participate in that sequencing, or the cadence tests
  go blind to them.
- Memory byte counts are `uint64` → `bigint` in TypeScript, matching `HostDiskStats.availableBytes`.
  `formatDiskFree(availableBytes: number | bigint)` already accepts both; a memory formatter follows
  that signature rather than forcing `Number()` at call sites.
- Proto field numbers must be genuinely free — never a reused retired number.
- Both regenerations required: `cargo build -p tddy-service`, `bun run --filter tddy-web generate`.

### User

- Every host row and the sessions footer gain a memory readout and, where the platform reports one, a
  load average. Hosts that cannot report load show `—`.

## Acceptance criteria

- [ ] **AC-1** An event carries total and available memory for the host.
- [ ] **AC-2** An event carries the logical core count explicitly.
- [ ] **AC-3** Where the platform reports one, an event carries the 1/5/15-minute load average.
- [ ] **AC-4** Where the platform does **not** report one, the event says so — and the UI renders `—`,
      never `0.00`.
- [ ] **AC-5** Memory and load refresh on the **fast** tick, at the same cadence as CPU.
- [ ] **AC-6** Disk continues to refresh on the slow tick, unchanged.
- [ ] **AC-7** Each Hosts row shows the host's memory.
- [ ] **AC-8** `HostStatsFooter` shows the new readings without regressing its existing indicators.

## Out of scope for this node

Per-mount disk reporting (only the project dir's filesystem is reported, as today). Any probe of
software installed on a host (nodes 4–8). Any change to the fan-out mechanics (node 2's).

## Successor PRs

- `feature/hosts-screen/host-identity` — each host's git identity and GitHub CLI status.
