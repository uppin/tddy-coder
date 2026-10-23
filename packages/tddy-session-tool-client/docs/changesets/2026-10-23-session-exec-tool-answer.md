# 2026-10-23 — Asking which exec tools the session is offered
**Type:** Feature

`session_exec_tool_answer()` asks `ListExecTools` for the session over every transport (HTTP, unix socket, sandbox socket, LiveKit) with a 10-second deadline, answering `Admitted`, `NoTransport` or `Failed`. See the [README](../../README.md#which-exec-tools-the-session-is-offered) (#700, `#project-home` 5/7).
