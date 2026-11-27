# 2026-11-28 — Launch sessions take the Fs handle (composition site)

**Type:** Refactoring · PR [#1051](https://github.com/uppin/tddy-coder/pull/1051) (`#fs-port` 7/13)

Composition-site wiring for the agents port: `LaunchSessions` gains `fs`
(`tddy-session-lifecycle` passes `DiskFs` when building launch sessions). No behaviour
change. Cross-package entry:
[`docs/dev/changesets/2026-11-28-fs-port-agents-fs.md](../../../../docs/dev/changesets/2026-11-28-fs-port-agents-fs.md).
