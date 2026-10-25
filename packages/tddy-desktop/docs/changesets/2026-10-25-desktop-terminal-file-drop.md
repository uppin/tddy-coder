# 2026-10-25 — Finder drops on a session terminal: the drag-step relay and the latest-drop file commands

**Type:** Fix

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`src-tauri/src/file_drop.rs`: `DroppedFiles` (only the latest drop's paths are readable), `tddy_dropped_files_describe` / `tddy_dropped_file_read` (raw bytes, 1 MiB cap), `NativeDragStep` / `drag_step_of` (CSS-pixel points; device pixels divided only on Windows). `lib.rs` relays every drag step as `tddy://native-file-drop` from `on_window_event`, recording a drop before emitting it. Detail: [file-drop.md](../file-drop.md).
