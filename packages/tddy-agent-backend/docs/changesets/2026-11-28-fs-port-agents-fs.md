# 2026-11-28 — Agent backends go through the Fs port

**Type:** Refactoring · PR [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)

Turn files, the usage tap, provider-route loading and the headless conversation log run
through `tddy-fs` behind held handles and `<fn>_on` seams. `tests/fs_port.rs` carries the
`direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-agents-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-agents-fs.md).
