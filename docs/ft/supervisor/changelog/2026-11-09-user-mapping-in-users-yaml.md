# 2026-11-09 — One users.yaml maps logins to OS users; one socket per user; a saved edit restarts the daemon

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- **`users.yaml` is the mapping.** `supervisor.yaml` names it in a top-level
  `users: {file, sockets_dir}` block, and the daemon service declares `serves_users: true`. The
  supervisor checks the file for trust like `supervisor.yaml`, refuses `root` and a login mapped to two
  OS users, and runs sessions only as an OS user it maps.
- **One socket per user.** Each mapped OS user gets `<sockets_dir>/<user>/daemon.sock`
  (default `/run/tddy-users`), handed to the daemon as `user.<user>` with the file's path in
  `TDDY_USERS_FILE`. It carries both the host-session services and the agent-tool roster, so up to
  **15** users fit one daemon.
- **A saved edit restarts the daemon**: unchanged users keep their descriptor, an invalid edit is
  refused and the running mapping kept, changes apply one at a time, and a daemon that survives
  `SIGTERM` and `SIGKILL` (10 s each) is reported rather than left on the old mapping. The restart
  drops live relayed agents; sessions resume from disk.
- **Upgrade.** `spawn_policy.allowed_session_users`, `host_sockets:` and `agent_tool_sockets:` fail
  loading with an error naming `users.yaml`. `./install` migrates once: `daemon.yaml` `users:` moves
  into `users.yaml`, the old keys are dropped, and a fresh install gets an empty `users.yaml` (`0644`).

See [tddy-supervisor.md § The user mapping](../tddy-supervisor.md#the-user-mapping-usersyaml).
