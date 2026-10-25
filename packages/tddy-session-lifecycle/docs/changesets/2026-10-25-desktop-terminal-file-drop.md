# 2026-10-25 — `ListSessions` reports whether a session reads host paths

**Type:** Fix

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`tests/list_sessions_managed_codebase_acceptance.rs` gains two cases: an unconfined claude-cli session is listed reading host paths, a managed one is not.
