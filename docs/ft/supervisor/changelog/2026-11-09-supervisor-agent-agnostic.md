# 2026-11-09 — The supervisor names no coding agent

`#agent-tools` 32/32 ([#865](https://github.com/uppin/tddy-coder/pull/865)).

- **Exec roots.** `spawn_policy.exec_roots` gates the binary of every spawn kind. An entry is a
  directory, a single trusted file, or `{ path, trusted_prefix: true }`. A binary runs only if the
  file it resolves to is under a root, and that file and every directory above it are root-owned and
  writable by nobody else. To grant a harness, install it under a root.
- **No env key list.** The daemon's environment passes whole, but loader variables are refused,
  `HOME`/`USER`/`LOGNAME` are forced, and `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` are stripped. An
  SSH clone or a commit identity needs no supervisor configuration.
- **Clones.** A clone is a `SpawnStreamed` of the `tddy-git-clone` helper, run as the session user.
  The supervisor never runs git and never touches the clone's files.
- **cgroup scopes.** Relayed and streamed spawns accept a scope. The daemon does not name one yet
  (`docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md`).

**Upgrade.** `allowed_tool_paths`, `allowed_relay_tool_paths` and `allowed_env_keys` now fail
loading. Re-running `./install` migrates them once: listed files become `exec_roots` entries and
`allowed_env_keys` is dropped. A daemon and a supervisor from different builds cannot clone. See
[tddy-supervisor.md § Exec roots](../tddy-supervisor.md#exec-roots).
