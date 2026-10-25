# Changeset: desktop-terminal-file-drop

PRD (target): [`docs/ft/web/web-terminal.md`](../../ft/web/web-terminal.md) § File drop upload —
*In Tddy Desktop* (amended 2026-10-25)

## Problem

In Tddy Desktop, dragging files from Finder onto a session terminal did nothing.

**Root cause.** `open_dashboard_window` (`packages/tddy-desktop/src-tauri/src/lib.rs`) builds the
window with Tauri v2's default `dragDropEnabled: true`. Its native handler (wry's
`WryWebView::draggingEntered` / `performDragOperation`) answers **every** drag itself and never
forwards it to WebKit, so the page gets no HTML5 `dragover` / `drop`. Tauri emitted
`tauri://drag-drop` instead, which nothing listened to. The terminal's existing
`TerminalFileDropZone` (HTML5, upload-then-type-path) therefore never ran in Desktop.

## Design decisions (settled with the developer)

- **Keep the native handler; route its events.** It is the only source of real paths. Disabling it
  would have restored the browser behaviour (upload a copy) but never give a local session the
  original path.
- **Real paths only where they mean something.** `SessionEntry.reads_host_paths` (new, proto field
  36) is computed by the daemon from `.session.yaml` (`tddy_changeset::reads_host_paths`): false for
  `sandbox`, `managed_codebase`, a split agent (`codebase_daemon_instance_id` — which also covers the
  sandboxed-codebase placement, so `sandboxed_codebase` needed no persisting), `ssh_config_host`, and
  `workspace` sessions. The web types real paths only when that flag is set **and** the session is
  on the application's own daemon (`sessionReadsLocalPaths`).
- **Everything else uploads** (the developer chose upload over refusal for remote/confined
  sessions), reading the bytes through two new host commands, `tddy_dropped_files_describe` and
  `tddy_dropped_file_read`, scoped to **the most recent drop's paths** and capped at 1 MiB per read.
- **Folders are refused with a visible message** (`terminal-drop-refused`); files beside them still
  upload.
- **Positions are normalised in Rust.** Tauri types drag positions as `PhysicalPosition`, but
  WKWebView (`draggingLocation`) and GTK report points; only WebView2 reports device pixels. The host
  re-emits each step as `tddy://native-file-drop` with CSS-pixel points (`file_drop::drag_step_of`,
  `cfg!(windows)`), so the page needs no platform knowledge. The drop is recorded before it is
  emitted, so the page can read the files as soon as it hears of them.
- **Dropped paths are pasted**, not typed: `GhosttyTerminalHandle.paste` → ghostty's `paste`, which
  wraps in bracketed-paste markers when DEC 2004 is on. This now applies to browser drops too.

## State B

- `packages/tddy-desktop/src-tauri/src/file_drop.rs`: `DroppedFiles` allowlist, the two commands,
  `NativeDragStep` + `drag_step_of`. `lib.rs` relays drag steps from `on_window_event`.
  `build.rs` and `capabilities/default.json` declare and grant the commands.
- `packages/tddy-tauri-web/src/nativeFileDrop.ts`: `NativeFileDropBridge` over the relay event and
  the commands.
- `packages/tddy-web`: `NativeFileDropProvider` (mounted in `index.tsx` only inside the host
  application) hit-tests drag points with `elementFromPoint` against registered terminals;
  `TerminalFileDropZone` registers, highlights, and delivers (paths / upload / refusal);
  `useSessionFileUpload.uploadSources` uploads from any byte source; `SessionRuntime` computes
  `readsLocalPaths` for the agent and bash terminals.
- Daemon: `SessionEntry.reads_host_paths` populated by `session_reader` / `ListSessions`.

## Known gaps (TODO, not in this changeset)

- **All in-page HTML5 drag-and-drop is swallowed in Desktop** for the same root cause — wry's handler
  claims every drag. Broken today and still broken: dragging a Files-tab entry onto the terminal
  (`HOST_PATH_MIME`), and dropping Finder files on the create-session `AttachmentDropZone`. The
  attachment zone can adopt `useNativeFileDropTarget`; the in-page drags need either a pointer-based
  drag or a Tauri-side passthrough for drags with no file paths.
- The mobile **Attach** button (`TerminalUploadButton`) still types uploaded paths with
  `sendInput`, not a paste.

## Package docs to update at wrap

- `packages/tddy-desktop/docs/` — a `file-drop.md` describing the relay event, the commands, the
  drop-scoped allowlist and the position-unit rule.
- `packages/tddy-web/docs/terminal-file-upload.md` — the native route, `uploadSources`, paste
  delivery.
- `packages/tddy-web/docs/terminal-session.md` — `readsLocalPaths` and `pasteInput`.

## Verification

- Rust: `tddy-changeset` (`reads_host_paths` cases), `tddy-desktop` (`file_drop` allowlist, read
  slicing/cap, drag-step mapping and serialisation), `tddy-session-lifecycle`
  (`list_sessions_managed_codebase_acceptance`: the listing reports the flag).
- Web: `TerminalNativeFileDrop.cy.tsx` (highlight, real paths, upload, folder refusal, routing to
  the terminal under the pointer, drop outside), `GhosttyTerminalSession.cy.tsx` (bracketed and
  plain paste of a drop), `sessionReadsLocalPaths.test.ts`.
