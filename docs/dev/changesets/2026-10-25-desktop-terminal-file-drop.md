# 2026-10-25 — Files dropped from Finder onto a Desktop session terminal reach the PTY

**Type:** Fix

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Packages: `tddy-desktop`, `tddy-tauri-web`, `tddy-web`, `tddy-changeset`, `tddy-session-activity`,
`tddy-agent-launch`, `tddy-service`.

**Defect.** In Tddy Desktop a file dragged from Finder onto a session terminal did nothing. The window
is built with Tauri v2's default `dragDropEnabled`; on macOS wry's native handler answers every drag
itself and never forwards one to WebKit, so the terminal's HTML5 `TerminalFileDropZone` never fired,
and the `tauri://drag-drop` Tauri emitted instead had no listener.

**Fix — the handler stays on, its events are routed.** It is the only source of a dropped file's real
path.

- `tddy-desktop` relays each drag step as `tddy://native-file-drop` with CSS-pixel points (Tauri's
  `PhysicalPosition` is points on WKWebView and GTK; only WebView2 reports device pixels), records
  the drop first, and serves the files of **only the latest drop** through
  `tddy_dropped_files_describe` / `tddy_dropped_file_read` (1 MiB per read).
- `tddy-web` hit-tests each point against registered terminals (`NativeFileDropProvider`), highlights
  the one under the pointer, and on drop pastes the real, shell-quoted paths into a session whose
  agent reads this machine's files; uploads the files into any other session; refuses folders with a
  visible message. Dropped paths go through the terminal's paste, bracketed when DEC 2004 is on, for
  browser drops too.
- The daemon reports `SessionEntry.reads_host_paths` (proto field 36), computed from `.session.yaml`
  by `tddy_changeset::reads_host_paths`.

Detail: [`tddy-desktop` file-drop.md](../../../packages/tddy-desktop/docs/file-drop.md),
[`tddy-web` terminal-file-upload.md](../../../packages/tddy-web/docs/terminal-file-upload.md),
[web-terminal.md § In Tddy Desktop](../../ft/web/web-terminal.md#file-drop-in-tddy-desktop).

**Deferred, to the backlog:** in-page HTML5 drags (Files-tab → terminal, Finder → create-session
attachments) are still swallowed by the window — `2026-10-25-desktop-swallows-in-page-drag-and-drop`;
the mobile Attach button still types uploaded paths without a paste —
`2026-10-25-mobile-attach-types-paths-without-paste`.

**Code issues measured at wrap (still open, not split here):** `tddy-web`
`GhosttyTerminalSession.tsx` 992 → 1006 lines and `GhosttyTerminal.tsx` 1354 → 1362 (budget 500);
recorded in their code-issue files.
