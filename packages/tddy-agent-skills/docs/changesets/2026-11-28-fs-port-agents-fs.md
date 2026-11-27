# 2026-11-28 — Agent skills go through the Fs port

**Type:** Refactoring · PR [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)

Skill scanning, slash-menu entries and skill-body reads go through `tddy-fs` behind `<fn>_on`
seams; old names delegate `DiskFs` for `tddy-core`/`tddy-coder` callers. `tests/fs_port.rs`
carries the `direct_disk_io_in` guard. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-agents-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-agents-fs.md).
