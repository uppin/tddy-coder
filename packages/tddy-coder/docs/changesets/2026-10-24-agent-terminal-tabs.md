# 2026-10-24 — Agent terminal tabs: the coder runs bash only
**Type:** Feature

`CoderTerminalRoster::start` takes a `TerminalLaunch`, runs `launch.login_shell` for `TerminalKind::Bash`, and refuses `TerminalKind::Agent` with `INVALID_ARGUMENT` "this session's terminals run bash only" — it runs no harness registry. Pinned by `start_terminal_session_refuses_an_agent_kind` (#718).
