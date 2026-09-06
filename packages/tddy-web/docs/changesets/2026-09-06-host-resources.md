# 2026-09-06 — Memory and load average in the host telemetry surface
**Type:** Feature

`useHostStats` surfaces `logicalCores`, `memory` and `load` beside the existing `perCorePercent` and
`disk`. A block absent from an event sets its reading back to `null` rather than to a zero, and
`load` is deliberately `null` both before the first frame and on a host whose platform has no load
average — the only honest rendering of either is "no reading".

`MemoryIndicator` and `LoadAverageIndicator` join `DiskSpaceIndicator` in the Host Stats Footer;
the Hosts row cell adds memory. Each renders an em dash for a null reading.

`hostStatsFormat.ts` keeps one byte-formatting body: `formatBytesFree`, which `formatDiskFree` now
delegates to. Memory and disk are the same quantity asked two ways, and a second near-identical
formatter is how two readouts drift apart.

See [`host-stats-streaming.md`](../host-stats-streaming.md).
