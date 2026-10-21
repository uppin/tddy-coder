# 2026-10-21 — a server that never came up is refused with its cause
**Type:** Fix

`LspError::ServerNotStarted { program, reason }`, raised by `get_or_spawn` for a server that could not be
spawned, exited before `initialize` answered, or failed the handshake; `ServerNotFound` is now raised for a
spawn `NotFound`. A server that came up and later died keeps `ServerExited`; `SPAWN_TIMEOUT` keeps `Timeout`.
No launch changes. Tests: `tests/server_start_failure_test.rs` (new `fake_lsp` modes),
`tests/spawn_observer_test.rs`. `tddy-index-daemon`'s `status_of_lsp` maps it to unavailable.
