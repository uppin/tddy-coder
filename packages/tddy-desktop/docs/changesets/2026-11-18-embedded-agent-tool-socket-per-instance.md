# 2026-11-18 — The application answers --help and stops its children on exit
**Type:** Fix

- `invocation.rs`: `--help`/`-h` and `--version`/`-V` print and exit, any other argument is refused,
  and `-psn_*` is ignored. Before this, `tddy-desktop --help` started a second instance.
- `stop_daemon` calls `DaemonChildren::shut_down`, which covers CLI sessions, workspace jails, the
  index daemon and the agent-tool socket file. `DaemonShutdown` holds `children` in place of
  `cli_sessions`.
