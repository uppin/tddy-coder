# 2026-11-21 — Session files an orchestrator writes

**Type:** Feature

- `tddy_session_files::session_files::SessionFileStore::{open, write, list}` keeps an orchestrator's
  files under `<session_dir>/artifacts/files/<name>`: one flat name (at most 128 bytes, no `/`, `..` or
  leading `.`), at most 1 MiB, written atomically through a hidden temporary `list` never shows.
- `tddy_tool_engine::session_file_tools` defines `WriteSessionFile` / `ListSessionFiles`, the
  `SessionFilePort` the daemon implements, and `attachment_for`: the `SESSION_ARTIFACT` `attachments[]`
  entry for a file. `StartSession` is unchanged — a session file is an ordinary session artifact.

See [session-attachments.md § Session files an orchestrator writes](../../../../docs/ft/coder/session-attachments.md#session-files-an-orchestrator-writes). Full record: [`docs/dev/changesets/2026-11-21-session-files.md`](../../../../docs/dev/changesets/2026-11-21-session-files.md).
