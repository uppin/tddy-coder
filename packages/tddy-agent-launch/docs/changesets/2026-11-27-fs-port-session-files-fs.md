# 2026-11-27 — Agent launch passes Fs to the session-files port (composition site)

**Type:** Refactoring · PR [#1049](https://github.com/uppin/tddy-coder/pull/1049) (`#fs-port` 5/13)

Composition-site wiring for the session-files port: `tddy-fs` dependency plus `DiskFs` at the
`AttachmentState` construction site. No behaviour change. Cross-package entry:
[`docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md](../../../../docs/dev/changesets/2026-11-27-fs-port-session-files-fs.md).
