# 2026-11-28 — Session activity goes through the Fs port

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

Session stores, the notification log/relay, the watch store and the activity service read and
write through `tddy-fs` behind `ActivityPorts.fs`, per-store handles and `<fn>_on` seams;
`DiskFs` stays at constructors and old-name wrappers. `tests/fs_port.rs` carries the
`direct_disk_io_in` guard plus the `MemFs` answer-wait round-trip. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
