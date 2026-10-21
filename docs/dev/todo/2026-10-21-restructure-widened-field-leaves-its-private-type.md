# 2026-10-21 — `move_item` widens a field to `pub(super)` but not the private type it names (E0446)

**Category:** Future enhancement (engine gap)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plan `12-host-tooling-subprocess`

## What happened

Plan `12-host-tooling-subprocess` (`move_item` + `name: subprocess`, `reexport: outside`) moves `ProbeOutput`,
`Written`, `StartedProbe`, `RunningProbe`, `start_probe` and the pipe plumbing out of
`packages/tddy-host-service/src/host_tooling.rs` into `host_tooling/subprocess.rs`. `restructure check --deep`
reported `no findings` and listed the widenings `StartedProbe::running` and `RunningProbe::child`
(private -> `pub(super)`). The apply then failed its compile gate:

```
packages/tddy-host-service/src/host_tooling.rs:442:9: error: type `RunningProbe` is private: private type
error: could not compile `tddy-host-service` (lib test) due to 1 previous error; 14 warnings emitted
```

The `host_tooling.rs` tests read `started.running` (type `Result<RunningProbe, String>`) and `.child`. The engine
widened the field but not `struct RunningProbe`, which the widened field names (E0446). The same failure
repeated on an engine rebuilt after the rebase onto master `35fcc585f`.

## Workaround used

The plan schema documents this class as a gate stop (a private field or method of a split type is not widened;
the run ends at the compile gate with the edit left on disk). The developer pre-approved one post-move build
fix (2026-10-21): `pub(super) struct RunningProbe` in `host_tooling/subprocess.rs`, marked with a `TODO`.
`apply --resume` re-ran nothing (`applied 0 of 1`), so the unused-import tidy and rustfmt were done with
`cargo fix --lib --tests -p tddy-host-service --allow-dirty` (13 unused imports in `subprocess.rs`) and
`cargo fmt -p tddy-host-service`.

## Fix

The engine should widen every private type a widened field or signature names, or `check --deep` should refuse
the move naming the type. Then drop the `pub(super)` and the TODO on `RunningProbe`.
