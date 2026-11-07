# The supervisor names no coding agent: exec roots, one env rule, a generic streamed spawn — PRD

**Date**: 2026-11-07
**PRD type**: Requirements change (existing feature)
**Stack**: `#agent-tools` 32/32 — branch `feature/agent-tools/supervisor-agent-agnostic`

## Affected Features

- [tddy-supervisor.md](../tddy-supervisor.md) — spawn policy, relayed spawns, `CloneRepository`, cgroup scopes
- [../daemon/systemd-install.md](../../daemon/systemd-install.md) — line 45 (`allowed_relay_tool_paths` and the harness names), plus the one-time migration
- [../daemon/session-agent-roster.md](../../daemon/session-agent-roster.md) — line 274 (relay needs `allowed_relay_tool_paths`)
- [../daemon/clone-ssh-passphrase.md](../../daemon/clone-ssh-passphrase.md) — lines 67, 122 and 140 (clone keys in `allowed_env_keys`)

## Summary

`tddy-supervisor` keeps its job: running `tddy-daemon`, and the processes the daemon asks for, as
the right OS user, inside cgroups. It stops knowing about coding agents, their environment
variables, or git. Three changes do that:

1. **Exec roots.**
   - One `spawn_policy.exec_roots` list replaces both `allowed_tool_paths` and
     `allowed_relay_tool_paths`.
   - A requested binary is run only if the file it resolves to lies under a root, and that file
     and every directory above it are owned by root (or the supervisor's euid) and are not
     group- or world-writable.
   - Granting a harness means installing it under a root.
2. **One env rule.**
   - `allowed_env_keys` goes. The env the daemon computed passes whole for every spawn kind:
     session, sandbox, streamed and relay.
   - Loader variables are still refused. `HOME`/`USER`/`LOGNAME` are forced to the target
     account's values. `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` are stripped.
3. **A generic streamed spawn.**
   - `CloneRepository` is replaced by `SpawnStreamed`: run a binary as a user, stream its stderr
     lines, and report its exit and the stderr tail.
   - The daemon composes `git clone`, parses its progress, and moves the finished clone into place
     **as the session user**.

Relayed agents and streamed spawns also get a **cgroup scope**, as sessions do.

## Background

Read at #836's tip `ddef5cc6b`:

- **Agent names in the supervisor.**
  - `allowed_relay_tool_paths` is documented as "coding-agent harness binaries"
    (`spawn_policy.rs:14-18`).
  - The templates list `codex-acp`, `opencode` and `claude-agent-acp`
    (`supervisor.yaml.production:103-116`, `dev.supervisor.yaml:68-74`).
  - The proto and relay docs call the relay a coding-agent harness.
- **The env allow-list is the daemon's knowledge, kept in the supervisor's file.**
  - `allowed_env_keys` (`spawn_policy.rs:22-29`, `policy.rs:145-169`) must list every key the
    daemon sends: the seven clone-SSH keys (`tddy-spawn/src/clone_env.rs:27-37`), the git commit
    identity, `TDDY_GITHUB_SOURCE`, and the route keys.
  - A missing key is a refusal at run time.
- **The binary lists are not a code-execution boundary.**
  - Arguments are never checked.
  - Once SSH cloning is configured, the env gate admits `GIT_SSH_COMMAND`, which is code
    execution through ssh.
  - The lists are defence in depth: an absolute path the root operator wrote down.
- **The supervisor runs git itself.**
  - `CloneRepository` finds git on its own `PATH` (`clone.rs:57-71`).
  - It parses git's stderr (`clone.rs:291-295`).
  - It renames the partial clone into place **as root**, in a directory the session user can
    write (`clone.rs:222-248`).
- **Relayed agents and clones run without a cgroup scope** (`server.rs:102-108`, `:244-250`).

The user's rule (2026-11-07): the supervisor "shouldn't be aware of any env or coding agent
specifics … any coding specific stuff should be configurable by YAML, so also allow-lists". The
daemon's spec YAML already names every harness binary.

## Proposed Changes

### What's Changing

- **`spawn_policy.exec_roots: [__INSTALL_BIN_DIR__]`.** The rules:
  - A requested path is plain and absolute, as today.
  - The file it resolves to must lie under a root.
  - That file and each parent directory up to `/` must be owned by uid 0 or the supervisor's
    euid, and must not be group- or world-writable. This reuses the trust checks of
    `config.rs:380-447`.
  - On Linux the **checked descriptor is what is exec'd** (`/proc/self/fd/<n>` from the `O_PATH`
    / `openat2` code at `spawn_broker.rs:520-572`), so nothing can be swapped in between the check
    and the exec.
  - The same gate applies to sessions, sandboxes, streamed spawns and relays. Relaying is a stdio
    mode, not a trust class of its own.
- **One env rule for every spawn kind.**
  - `allowed_env_keys` and `CloneSshEnv::ALLOWED_ENV_KEYS` are deleted.
  - The daemon's env passes whole, except:
    - loader variables, which are refused;
    - `HOME`, `USER` and `LOGNAME`, which are set from the target account;
    - `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET`, which are stripped.
  - The base env is:
    - `Minimal` (fixed `PATH`, `LANG`, `TZ`) for sessions, sandboxes and streamed spawns;
    - `Exactly` for relays, as #836 made it.
  - No runtime-specific names (`NODE_OPTIONS`, …) are added to the loader list.
- **`SpawnStreamed` replaces `CloneRepository`.** The request carries the binary, args, env, user,
  working directory and scope. The stream reports `Started{pid}`, one `Line` per stderr line
  (split on `\r` and `\n`), then `Exited{code, signal, stderr_tail}`. The daemon:
  - composes `git clone --progress -- <url> <partial>`;
  - parses progress with `tddy-spawn/src/git_progress.rs`;
  - does the partial → destination rename, and the removal on failure, as the session user.
- **cgroup scopes** for relayed spawns and streamed spawns, as `SpawnSession` takes one.
- **Old keys are load errors** naming their replacement:
  - `allowed_tool_paths` and `allowed_relay_tool_paths` → `exec_roots`;
  - `allowed_env_keys` → removed.

  `./install` migrates them once, extending #859's migration.
- The templates and docs name no coding agent.

### What's Staying the Same

- Who a process may run as: `users.yaml` (#859). `root` is never run as.
- `allowed_mount_roots` and the sandbox jail.
- The loader-variable refusal.
- `SpawnSession`, `SpawnSandbox`, `SpawnRelayed` and the session table semantics
  (`StopSession`, `SessionStatus`).
- The opaque refusal to the caller. The supervisor logs the reason.

## Impact Analysis

### Technical Impact

- `tddy-supervisor`: policy, config, server, the spawn broker's exec, the relay, the proto,
  `clone.rs` (deleted or reduced to line splitting), the cgroups for new spawn kinds, the
  templates, and the architecture doc.
- `tddy-spawn`: `clone_env.rs`, `supervisor_spawn.rs` (the clone over `SpawnStreamed`),
  `clone_run.rs`, and `supervisor_client.rs`.
- `tddy-session-lifecycle`: the relay refusal text (`coding_agent_relay.rs`).
- `tddy-daemon-rpc`: the tests in `supervisor_clone_delegation.rs`.
- The `./install` script and `tddy-e2e` install tests.

### User Impact

- An operator no longer lists harness binaries or environment keys. A harness installed under the
  install bin dir just works. One installed elsewhere is refused at run time, with the reason in
  the supervisor's log.
- **An existing `supervisor.yaml` fails to load until it is migrated.** `./install` does that
  once.
- **A supervisor and daemon from different builds stop talking for clones**, because
  `CloneRepository` is gone.

## Acceptance Criteria

- [ ] A binary under an exec root, root-owned and not group- or world-writable all the way up, is run. One outside every root, or with a writable file or parent, is refused, and the log names the reason.
- [ ] On Linux, swapping the file after the check does not change what is exec'd.
- [ ] Session, sandbox, streamed and relayed spawns all pass the daemon's env whole, refuse a loader variable, force `HOME`/`USER`/`LOGNAME`, and strip `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET`.
- [ ] An SSH clone needs no supervisor env configuration.
- [ ] A clone through the supervisor reports git's progress, parsed by the daemon. The clone ends up at its destination, owned by the session user, renamed by a process running as that user. A failed clone leaves no partial directory.
- [ ] Relayed and streamed spawns run in a cgroup scope.
- [ ] `allowed_tool_paths`, `allowed_relay_tool_paths` or `allowed_env_keys` in `supervisor.yaml` fails loading, naming the replacement. `./install` migrates them once.
- [ ] No coding-agent name appears in `tddy-supervisor`'s `src/`, `proto/`, or the supervisor templates.
- [ ] The daemon's relay refusal names `exec_roots` and the env rule, not the removed keys.

## References

- Changeset: `docs/dev/1-WIP/2026-11-07-supervisor-agent-agnostic.md`
- Predecessors:
  - `#agent-tools` 28 (#836): `resolve_relayed_env`, `EnvironmentBase::Exactly`;
  - `#agent-tools` 31 (#859): `users.yaml` and the install migration.
