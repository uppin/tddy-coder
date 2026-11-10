# 2026-11-09 — Exec roots, one env rule and SpawnStreamed: the supervisor names no coding agent

**Type:** Architecture

`#agent-tools` 32/32 ([#865](https://github.com/uppin/tddy-coder/pull/865)).

- `SpawnPolicy.exec_roots: Vec<ExecRoot>` (`config/spawn_policy.rs`) replaces `allowed_tool_paths`
  and `allowed_relay_tool_paths`. An entry is a bare absolute path or `{ path, trusted_prefix }`.
  `policy::resolve_exec` returns a `CheckedExecutable` and implements:
  - a symlink is judged by its target;
  - the file, every parent and the entry must be root- or euid-owned and not group- or
    world-writable, a sticky world-writable directory included;
  - a trusted-prefix entry's own directory is not checked.

  On Linux `ForkBroker` execs `/proc/self/fd/<n>` with `argv[0]` set to the checked path; macOS
  checks the path only. The reason for a refusal is logged under the `tddy_supervisor::policy` target.
- `policy::resolve_spawn_env(SpawnKind, env)` is the one env rule. It is `Minimal` for
  `Session`/`Sandbox`/`Streamed` and `Exactly` for `Relayed`, and `allowed_env_keys` is gone. All four
  spawn methods pass `PrivilegedSurface::resolve_spawn(kind, …)` (`server.rs`).
- `spawn_policy::refuse_old_spawn_policy_keys` fails a config that names `allowed_tool_paths`,
  `allowed_relay_tool_paths` or `allowed_env_keys`, and names the replacement.
- `rpc SpawnStreamed` (`SpawnStreamedRequest`, `StreamedEvent` = `Started` / `Line` / `Exited`)
  replaces `CloneRepository`, and `src/streamed.rs` replaces `src/clone.rs`. There is no git
  heuristic and no root rename. `SupervisorClient::spawn_streamed` is the client.
  `SpawnRelayedRequest.scope` is field 7.
- `supervisor.yaml.production` and `dev.supervisor.yaml` name no coding agent. The guard is
  `tests/supervisor_names_no_coding_agent.rs`.
- Tests:
  - `policy.rs` covers the exec gate, symlinks, trust, trusted prefixes, the Linux fd swap and macOS
    path-only.
  - `spawn_broker.rs` covers the env rule for each kind and loader refusal.
  - `config.rs` covers the old keys and non-absolute roots.
  - `tests/supervisor_streamed_spawn.rs` and `tests/supervisor_relay.rs` cover the streamed spawn
    and its cgroup scope.

  The integration fixtures live under `CARGO_TARGET_TMPDIR`, not `/tmp`.
- Code issues: `oversized-file-spawn-broker` stayed at 984 production lines. `server.rs` went from 419
  to 363 production lines, and its record was already closed by #721.
- Docs: [architecture.md](../architecture.md) and [host-sockets.md](../host-sockets.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-supervisor-agent-agnostic.md).
