# 2026-11-09 — The supervisor names no coding agent: exec roots, one env rule, a generic streamed spawn

**Type:** Architecture

`#agent-tools` 32/32 ([#865](https://github.com/uppin/tddy-coder/pull/865)).

`tddy-supervisor` gates where a binary lives and who it runs as, and knows nothing about coding
agents, their environment or git.

- **Exec roots** (`tddy-supervisor`). `spawn_policy.exec_roots` replaces `allowed_tool_paths` and
  `allowed_relay_tool_paths` for every spawn kind. An entry is an absolute directory or single file,
  or `{ path, trusted_prefix: true }`. A symlink is judged by its target. The file, every parent and
  the entry must be owned by root or the supervisor's euid and not group- or world-writable, a sticky
  world-writable directory included. Linux execs the checked `O_PATH` descriptor; macOS checks the
  path only. A refusal is the opaque `Denied`, and the reason is logged.
- **One env rule** (`policy::resolve_spawn_env`). The daemon's env passes whole. Loader variables are
  refused, `HOME`/`USER`/`LOGNAME` are forced, and `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` are
  stripped. The base is `Minimal` for session, sandbox and streamed spawns and `Exactly` for relayed
  ones. `allowed_env_keys` and `CloneSshEnv::ALLOWED_ENV_KEYS` are deleted. Every spawn method passes
  one `PrivilegedSurface::resolve_spawn(kind, …)`.
- **`SpawnStreamed` replaces `CloneRepository`** (proto). It reports `Started{pid}`, then `Line` per
  stderr line, then `Exited{code, signal, stderr_tail}`. `src/clone.rs` is replaced by
  `src/streamed.rs`.
- **cgroup scope for relayed and streamed spawns**: delivered on the supervisor side.
  `SpawnRelayedRequest.scope` (field 7) and `SpawnStreamedRequest.scope` place either child in an
  existing scope before it execs. The daemon side is deferred by the user (2026-11-09, #865 wrap):
  the daemon creates and destroys no session scope yet, so it names none, as for sessions and
  clones.
- **The clone is the daemon's** (`tddy-spawn`, new crate `tddy-git-clone`). `plan_supervised_clone`
  builds one `SpawnStreamed` of `tddy-git-clone --git <git> -- <url> <destination>` as the session
  user. The helper clones into a partial sibling, renames it into place, removes it on failure and
  refuses an existing destination. The daemon parses git's progress. A missing git or helper is a
  "failed precondition" refusal. `./release`, `./install`, `./publish.sh` and
  `scripts/dev-runtime-binaries.sh` ship the helper.
- **Config migration** (`tddy-supervisor`, `./install`). The three old keys fail loading and name
  their replacement. `./install`'s `migrate_spawn_policy_keys` migrates them once. A fresh install
  renders a live `exec_roots`: the bin dir, the resolved git file, and `/nix/store` as a trusted
  prefix only when git resolves there. A missing git fails the render.
- **No agent names** in the supervisor's non-test source, its proto, `supervisor.yaml.production` or
  `dev.supervisor.yaml`. The guard is `tests/supervisor_names_no_coding_agent.rs`.
- **Daemon relay refusal** (`tddy-session-lifecycle`, `coding_agent_relay.rs`). It names
  `exec_roots`, the session user and the loader-variable rule.

**Backlog.** Added:
- `docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md`. Deferred by the
  user, 2026-11-09: the supervisor accepts scopes on relayed and streamed spawns; the daemon creating
  and destroying session scopes is a follow-up node.
- `docs/dev/todo/2026-11-08-install-renders-configured-harness-exec-roots.md`. Harness binaries
  outside the bin dir are operator entries.

None was resolved. `2026-11-01-install-script-over-length-budget` was narrowed with this PR's
measurement: `./install` grew from 1,600 to 1,811 lines.

**Code issues.** `tddy-supervisor/oversized-file-spawn-broker` stayed at 984 production lines, and
a row records that.

Per package:
- [tddy-supervisor](../../../packages/tddy-supervisor/docs/changesets/2026-11-09-supervisor-agent-agnostic.md)
- [tddy-spawn](../../../packages/tddy-spawn/docs/changesets/2026-11-09-supervisor-agent-agnostic.md)
- [tddy-session-lifecycle](../../../packages/tddy-session-lifecycle/docs/changesets/2026-11-09-supervisor-agent-agnostic.md)

Feature: [tddy-supervisor.md](../../ft/supervisor/tddy-supervisor.md).
