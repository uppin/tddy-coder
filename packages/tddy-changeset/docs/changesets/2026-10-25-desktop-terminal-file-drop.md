# 2026-10-25 — `reads_host_paths`: whether a session's agent reads this host's filesystem

**Type:** Feature

PR [#765](https://github.com/uppin/tddy-coder/pull/765). Cross-package entry: [`docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md`](../../../../docs/dev/changesets/2026-10-25-desktop-terminal-file-drop.md)

`session_metadata::reads_host_paths(&SessionMetadata)` — false for `sandbox`, `managed_codebase`, a non-blank `codebase_daemon_instance_id` or `ssh_config_host`, and a `workspace` session. Re-exported from the crate root (and through `tddy_core`).
