# 2026-11-06 — The host as a session starter

**Type:** Feature

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- `connection_service::operator_session_starter`: `HostSessionStarter` over `start_session_for_os_user`, `operator_start_request` (`on_branch_conflict = "reject"`, no token) and `operator_start_outcome`.
- Suites that launched through `CliSessionManager::start` moved to `start_with_options` (approved, 2026-11-06); `cursor_cli_session_acceptance.rs` lost its `resume_cursor` cases.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).
