# 2026-11-28 — Spawn goes through the Fs port

**Type:** Refactoring · PR [#1052](https://github.com/uppin/tddy-coder/pull/1052) (`#fs-port` 8/13)

Spawner entry points, clone runs and child log setup read and write through `tddy-fs` behind
`<fn>_on` seams; the worker's pipe ends are a dedicated `PipeEnd` fd type, and child stdio
stays on `OwnedFd`. `tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package
entry:
[`docs/dev/changesets/2026-11-28-fs-port-supervisor-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-supervisor-fs.md).
