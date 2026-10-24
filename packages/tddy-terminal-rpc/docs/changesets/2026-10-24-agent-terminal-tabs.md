# 2026-10-24 — Agent terminal tabs: `StartTerminalSession` takes a `kind`
**Type:** Feature

`StartTerminalSessionRequest.kind = 4`. The handler parses it with `TerminalKind::from_request`
(`""`/`"bash"` → `Bash`, else `Agent(kind)`) and calls `TerminalRoster::start(session_id, &TerminalLaunch { kind, login_shell })`,
which replaces `start(session_id, shell_path)`. The port types move to `service::ports`
(`src/service/ports.rs`), re-exported from `service` so their paths are unchanged. The in-memory test
roster records launches (`started_launches()`). See [terminal-session-service.md](../terminal-session-service.md) (#718).

Restructuring (pr-wrap file-length gate): `src/service.rs` was 575 production lines at the base and
609 after this change; one `move_item` (`tddy-tools restructure`) moved the ten port items into
`src/service/ports.rs` (137 lines), leaving `service.rs` at 483.
