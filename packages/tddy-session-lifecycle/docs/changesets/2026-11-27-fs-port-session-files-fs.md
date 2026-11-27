# 2026-11-27 — Session files ports take the Fs handle (composition sites)

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Composition-site wiring for the session-files port: `SessionFilesPorts` gains `fs`
(`tddy-session-lifecycle` passes `DiskFs`), `AttachmentState` gains `fs`
(`tddy-agent-launch`, `tddy-session-split` pass `DiskFs`), the lifecycle host builder passes
`DiskFs` for the worktree size calculator, and `tddy-session-agents` passes `DiskFs` for the
sync `Mirror`. No behaviour change. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
