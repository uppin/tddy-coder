# 2026-09-06 — `logicalCores` is surfaced but consumed nowhere

**Category:** Future enhancement
**Source:** `#hosts-screen` 3/8, PR #455

- `HostStatsEvent.cpu.logical_cores` reaches `UseHostStatsResult.logicalCores`, and nothing renders
  it — `CpuCoresIndicator` still derives its bars from `perCorePercent`.
- Deliberate: no test asks for it, and speculative rendering was not added. Either give it a
  consumer or drop it from the hook's result type; leaving an exposed reading nothing reads is the
  state to resolve.
