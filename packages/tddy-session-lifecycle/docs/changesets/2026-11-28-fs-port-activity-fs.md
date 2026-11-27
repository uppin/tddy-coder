# 2026-11-28 — Activity ports take the Fs handle (composition site)

**Type:** Refactoring · PR [#1050](https://github.com/uppin/tddy-coder/pull/1050) (`#fs-port` 6/13)

Composition-site wiring for the activity port: `ActivityPorts` gains `fs`
(`tddy-session-lifecycle` passes `DiskFs` when building the activity service). No behaviour
change. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-activity-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-activity-fs.md).
