# 2026-11-15 — Session names in `.session.yaml`

**Type:** Feature

`SessionMetadata.display_name` (absent for an unnamed session, never written then) and the `session_display_name` module: `DISPLAY_NAME_MAX_CHARS` (80 characters), `normalize_display_name` (trim; empty clears; `TooLong` / `ControlCharacter` refused, never cut) and `rename_session`, which changes the name and nothing else. `session_display_label` takes `display_name` first. See [architecture.md § Session name](../architecture.md#session-name-session_display_name).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
