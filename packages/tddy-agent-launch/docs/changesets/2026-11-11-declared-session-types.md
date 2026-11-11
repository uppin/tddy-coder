# 2026-11-11 — Session types come from YAML declarations

**Type:** Fix

- Start and resume dispatch on `harness_from_spec::SessionLaunch { NoAgent, AgentCli, TddyCoder }`: `requested_session_launch` (start: empty or undeclared → `INVALID_ARGUMENT` naming the offered types; checked in `start_session_on_request`, `start_here_body` and `refuse_a_start_before_it_begins`) and `recorded_session_launch` (resume and `ConnectSession`; legacy record with a `tool` → `TddyCoder`, `TODO(session-types)`).
- `refuse_a_tool_path_the_daemon_does_not_allow`: a `tddy_coder` start's empty or unlisted `tool_path` is refused naming `allowed_tools`.
- `runs_in_tmux`, `refuse_an_orchestrated_tool_start`, `refuse_a_type_main_cannot_run` and `profile_start` read the declarations (a tool-session profile gets the sole `tddy_coder` session type). `no_coding_agents()` holds the built-in session tools.
- Fix: the sandboxed CLI start looks its spec up by `kind.spec_id`, so a jailed Cursor session gets its `.cursor/hooks.json`.
- Docs: [module layout § Session start](../module-layout.md#session-start).

Cross-package: [declared session types](../../../../docs/dev/changesets/2026-11-11-declared-session-types.md). Product: [Session types are declared](../../../../docs/ft/daemon/session-agent-roster.md#session-types-are-declared).
