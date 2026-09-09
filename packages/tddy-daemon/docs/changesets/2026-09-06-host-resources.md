# 2026-09-06 — Memory, load average and core count on the host stats stream
**Type:** Feature

`HostStats` reports three further readings: `memory()` (total/available bytes), `logical_cores()`,
and `load_average()` returning `Option<LoadAverage>`. `HostStatsEvent` carries them as a
`HostMemoryStats` block, a `logical_cores` field on `HostCpuStats`, and an optional `HostLoadStats`
block.

Memory and load ride the existing **fast** (5 s) tick with CPU, because they move on the same
timescale; disk stays on the slow (60 s) tick, since enumerating mounts is the expensive read. No
third timer and no third builder parameter were added.

`SysinfoHostStats::load_average` resolves the platform at **compile time**: `sysinfo` implements a
real load average only for macOS, iOS, Linux, Android and FreeBSD and returns an all-zero `LoadAvg`
elsewhere, so every other target returns `None` and the zero sentinel never reaches the wire.

Breaking the `HostStats` trait was chosen over defaulted methods so the compiler enumerates every
implementor, including the in-test doubles — a double silently reporting a stale shape is the failure
a default would have allowed.

See [`connection-service.md`](../connection-service.md).
