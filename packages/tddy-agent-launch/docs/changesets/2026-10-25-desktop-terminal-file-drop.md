# 2026-10-25 — `ListSessions` carries `reads_host_paths`

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`session_coordinate_handlers` copies the reader's `reads_host_paths` onto the proto `SessionEntry`.
