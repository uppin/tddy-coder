# 2026-10-21 — `move_item` re-points imports of `#[cfg(unix)]` items without their `#[cfg]`

**Category:** Future enhancement (engine gap)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plans `11-host-tooling-readings` and `12-host-tooling-subprocess`, found by `/validate-changes`

## What happened

Plans 11 and 12 (`move_item` + `name`, `reexport: outside`) moved `#[cfg(unix)]` items out of
`packages/tddy-host-service/src/host_tooling.rs` into `host_tooling/readings.rs` and `host_tooling/subprocess.rs`.
The items kept their `#[cfg(unix)]`; the `use` lines the engine wrote to re-point their callers did not:

```rust
// packages/tddy-host-service/src/github_cli.rs, origin/master
#[cfg(unix)]
use crate::host_tooling::{start_probe, summarise, ProbeOutput, PROBE_TIMEOUT};

// after plan 12
use crate::host_tooling::subprocess::start_probe;   // unconditional
use crate::host_tooling::subprocess::ProbeOutput;   // unconditional
#[cfg(unix)]
use crate::host_tooling::{summarise, PROBE_TIMEOUT};
```

The same happened to the imports plan 11–12 added at the top of `host_tooling.rs` and in `readings.rs`. On a
unix target nothing changes; on any other target those imports name items that are compiled out (unresolved
import). Neither `check --deep`, the apply's compile gate, the tests nor clippy can see it on a unix machine, and
CI runs only Linux.

## Workaround used

The `#[cfg(unix)]` was restored by hand on each such `use` line (hand edit E5 of #693, in scope as a
behaviour-preserving fix).

## Fix

When `move_item` re-points or adds a `use` for an item carrying a `#[cfg(...)]`, the engine should copy the
item's `cfg` onto the import (or onto the import of the item it splits out of a cfg'd `use` group), so the moved
tree compiles on exactly the targets the original did.
