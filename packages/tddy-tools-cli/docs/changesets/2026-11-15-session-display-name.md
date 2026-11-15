# 2026-11-15 — The `rename-session` subcommand

**Type:** Feature

`Subcommand::RenameSession` dispatches to `tddy_tools::rename_session_cli`. `tests/rename_session_cli_acceptance.rs` runs the binary from the temp directory, so a session pointer above the checkout is not adopted.

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
