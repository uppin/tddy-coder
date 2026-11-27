# 2026-11-28 — Supervisor goes through the Fs port

**Type:** Refactoring · PR [#1052](https://github.com/uppin/tddy-coder/pull/1052) (`#fs-port` 8/13)

Config loading, the cgroup broker, socket binding and the relay read and write through
`tddy-fs` behind `Supervisor.fs` and held handles; descriptor-bound operations stay on
`libc`/`OwnedFd`. `tests/fs_port.rs` carries the `direct_disk_io_in` guard. Cross-package
entry:
[`docs/dev/changesets/2026-11-28-fs-port-supervisor-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-supervisor-fs.md).
