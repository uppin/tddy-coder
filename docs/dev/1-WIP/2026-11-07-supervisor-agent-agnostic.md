# Changeset: The supervisor names no coding agent: exec roots, one env rule, a generic streamed spawn

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5. It spans three crates (supervisor, spawn, session-lifecycle), plus tests in daemon-rpc and e2e and the install script. It changes the proto contract (`CloneRepository` → `SpawnStreamed`, plus scopes on relayed and streamed spawns), migrates config data, and changes a security gate (exec roots, fd exec). Several defaults are still unconfirmed.

## Initial Discovery

[2026-11-07-supervisor-agent-agnostic-initial-discovery.md](./2026-11-07-supervisor-agent-agnostic-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-01-install-script-over-length-budget.md](../todo/2026-11-01-install-script-over-length-budget.md) | **During.** Another one-time migration step is added to `./install`. Measure it at wrap and narrow the record; #859 also measures it. |
| [2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md](../todo/2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md) | **Reference.** The supervisor already forces the account's `HOME`/`USER`/`LOGNAME` on a relayed spawn. That record covers the daemon's own `setpriv` path, which this node does not touch. |
| [2026-08-03-tddy-supervisor-deliberate-gaps-and-follow-ups.md](../todo/2026-08-03-tddy-supervisor-deliberate-gaps-and-follow-ups.md) | **Reference.** Its spawn-surface follow-ups are unchanged. Pass 2 re-checks it for any entry that names `allowed_tool_paths` or `CloneRepository`. |

No `Claimed by:` record exists in `tddy-supervisor`, `tddy-spawn` or `tddy-session-lifecycle` code
issues for the touched files.

## Affected Packages

- **tddy-supervisor**:
  - `config/spawn_policy.rs`:
    - `exec_roots: Vec<PathBuf>` replaces `allowed_tool_paths`, and `allowed_relay_tool_paths` goes (:11-18);
    - `allowed_env_keys` goes (:22-29);
    - the three old keys fail loading, naming the replacement.
  - `config.rs`:
    - validation (:349-367) checks that the `exec_roots` entries are absolute;
    - the trust checks (`require_trustworthy_config_file` and `require_trustworthy_config_directory`, :380-447) are shared with the exec gate.
  - `policy.rs`:
    - `resolve_tool_path` and `resolve_relay_tool_path` (:98-131) become one `resolve_exec(policy, path)`, which checks the root and the trust of the file and every parent;
    - `resolve_env` (:145-169) and `resolve_relayed_env` (#836) become one rule.
  - `server.rs`:
    - `spawn_relayed` (:89-108) uses the same exec gate and takes a scope;
    - `resolve_spawn_of` (:190-230) applies the env rule;
    - `clone_repository` (:232-288) is replaced by `spawn_streamed`.
  - `spawn_broker.rs`:
    - the `SESSION_PATH` and `FORWARDED_VARIABLES` doc (:150-160) is rewritten;
    - on Linux, the binary is exec'd through the checked `O_PATH` descriptor (`open_resolved`, :551-572).
  - `clone.rs`: deleted. Its line splitting (`read_lines_in_the_background` :265, `for_each_line` :300) moves to the streamed spawn, without git's `%` heuristic, which moves to the daemon.
  - `proto/supervisor.proto`:
    - `rpc CloneRepository` (:44-48) and its messages (:146-181) are replaced by `SpawnStreamed`;
    - `SpawnRelayedRequest` gains `scope`;
    - the harness wording (:27) goes.
  - `relay.rs`: the module doc (:3).
  - `docs/architecture.md`.
  - `tests/supervisor_call_deadline.rs:119`: the `/usr/bin/claude` fixture is renamed.
- **tddy-spawn**:
  - `clone_env.rs` (:1-37): `ALLOWED_ENV_KEYS` and the module doc go.
  - `supervisor_spawn.rs`:
    - the module doc;
    - `clone_repo_via_supervisor_with_progress` (:250-262 and on) is rewritten over `SpawnStreamed`, with progress parsed by `git_progress.rs`;
    - the rename is done as the session user.
  - `supervisor_client.rs`: the client for the new RPC.
- **tddy-session-lifecycle**: `coding_agent_relay.rs`, whose refusal text (:9-11, :101-111) and tests (:322, :340) name the removed keys. #859 owns the `allowed_session_users` → `users.yaml` wording at :331.
- **tddy-daemon-rpc**: `tests/supervisor_clone_delegation.rs` is re-pointed at `SpawnStreamed`.
- **tddy-e2e**: `tests/install_supervisor.rs` and `tests/install_script.rs` cover the migration and the templates.
- **Non-Rust**:
  - `supervisor.yaml.production` (:98-140): no agent names; `exec_roots`; no env key lists.
  - `dev.supervisor.yaml` (:64-87).
  - The one-time migration in `install`.
  - Feature docs: `docs/ft/supervisor/tddy-supervisor.md`, `docs/ft/daemon/systemd-install.md:45`, `docs/ft/daemon/session-agent-roster.md:272-276`, `docs/ft/daemon/clone-ssh-passphrase.md:67,122,140`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-supervisor-agent-agnostic.md](../../ft/supervisor/1-WIP/PRD-2026-11-07-supervisor-agent-agnostic.md)

## Summary

The supervisor gates *where* an executable lives and *who* it runs as, and puts every child in a
cgroup scope. It does not know what the executable is, which env variables it needs, or how git
works.

- Exec roots replace both binary lists.
- One env rule replaces `allowed_env_keys` for every spawn kind.
- A generic `SpawnStreamed` replaces `CloneRepository`. With it, the daemon parses git's progress
  and renames the clone as the session user.

## Scope

- [ ] `spawn_policy.exec_roots` replaces `allowed_tool_paths` and `allowed_relay_tool_paths`: resolved file under a root, file and every parent root-owned (or euid) and not group/other-writable
- [ ] Linux: exec the checked descriptor; macOS: path-only check (tests say so)
- [ ] One env rule for session, sandbox, streamed and relay:
  - pass whole;
  - refuse loader variables;
  - force `HOME`/`USER`/`LOGNAME`;
  - strip `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET`.

  Base is `Minimal` for session/sandbox/streamed and `Exactly` for relay. `allowed_env_keys` and `CloneSshEnv::ALLOWED_ENV_KEYS` are deleted.
- [ ] `SpawnStreamed` replaces `CloneRepository` (proto). The daemon composes `git clone`, parses progress and renames as the session user.
- [ ] cgroup scope for relayed and streamed spawns
- [ ] Old keys are load errors naming the replacement. `./install` migrates them once (extends #859's migration).
- [ ] Templates, proto, relay and architecture docs name no coding agent. Guard `the_supervisor_names_no_coding_agent`.
- [ ] Daemon relay refusal text (`coding_agent_relay.rs`) names the new gate
- [ ] Feature docs updated (`tddy-supervisor.md`, `systemd-install.md`, `session-agent-roster.md`, `clone-ssh-passphrase.md`)

## Technical Changes

### State A

These are the 11 items found by the read-only discovery at `ddef5cc6b`, re-verified (see the
discovery file, Exploration 2):

1. `allowed_relay_tool_paths`:
   - `spawn_policy.rs:14-18`;
   - `config.rs:352-354`;
   - `policy.rs:114-131`;
   - `server.rs:94-99`.
2. Agent names in the templates (`supervisor.yaml.production:103-116`, `dev.supervisor.yaml:68-74`).
3. `allowed_env_keys`:
   - `spawn_policy.rs:22-29`;
   - `config.rs:358-367`;
   - `policy.rs:145-169`;
   - `server.rs:200-203`.
4. The askpass and commit-identity key lists in the templates (prod :117-140, dev :76-87).
5. #836's `resolve_relayed_env` (`policy.rs:171` and on) and `EnvironmentBase::Exactly`.
6. The `SESSION_PATH` and `FORWARDED_VARIABLES` doc (`spawn_broker.rs:150-160`).
7. `CloneRepository` knows git:
   - `clone.rs:1-12` (module doc);
   - `clone.rs:57-71` (git on the supervisor's `PATH`);
   - `clone.rs:222-248` (`move_into_place`, the rename as root);
   - `clone.rs:291-295` (the progress heuristic);
   - `server.rs:232-288`;
   - `supervisor.proto:44-48` and `:146-181`.
8. The proto and relay docs call the relay a coding-agent harness (`supervisor.proto:27`, `relay.rs:3`).
9. The sockets and `allowed_session_users` are #859's, not this node's.
10. Relayed agents and clones get no cgroup scope (`server.rs:106`, `:247-248`, where `scope` is passed as `None`).
11. `allowed_tool_paths` (`spawn_policy.rs:11-13`, `policy.rs:98-112`).

### State B

See the PRD's "What's Changing".

## Testing Plan

- Unit tests in `tddy-supervisor` for the exec gate (temp trees with a chosen owner and mode, the
  current user standing in for root, as `config.rs`'s trust tests do), the env rule and the
  config load errors.
- Linux-only integration tests for the fd exec and the end-to-end relayed and streamed spawns, as
  `tests/supervisor_relay.rs` already is.
- Daemon-side tests in `tddy-spawn` and `tddy-daemon-rpc` for the clone over `SpawnStreamed`.
- A source scan as the guard.
- e2e install tests for the migration.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `a_binary_under_an_exec_root_owned_by_root_is_run` | `packages/tddy-supervisor/src/policy.rs` (`#[cfg(test)]`) | 🔴 red |
| `a_binary_outside_every_exec_root_is_refused` | same | 🔴 red |
| `a_binary_whose_file_or_any_parent_is_group_or_world_writable_is_refused` | same | 🔴 red |
| `a_binary_owned_by_another_user_is_refused` | same | 🔴 red |
| `a_symlink_is_judged_by_its_target_which_must_lie_under_a_root` (default to confirm) | same | 🔴 red |
| `the_spawn_env_rule` — `#[rstest]` over `session`, `sandbox`, `streamed`, `relay`: passes the daemon env whole; refuses `LD_PRELOAD`; forces `HOME`/`USER`/`LOGNAME`; strips `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET`; base `Minimal` or `Exactly` | `packages/tddy-supervisor/src/policy.rs` / `spawn_broker.rs` (`#[cfg(test)]`) | 🔴 red |
| `an_old_spawn_policy_key_fails_loading_naming_its_replacement` — `#[rstest]` over `allowed_tool_paths`, `allowed_relay_tool_paths`, `allowed_env_keys` | `packages/tddy-supervisor/src/config.rs` (`#[cfg(test)]`) | 🔴 red |
| `an_exec_root_that_is_not_absolute_fails_loading` | same | 🔴 red |
| `a_file_swapped_after_the_check_is_not_what_is_execd` (Linux only) | `packages/tddy-supervisor/tests/` | 🔴 red |
| `a_relayed_spawn_runs_in_a_cgroup_scope` (Linux only) | `packages/tddy-supervisor/tests/supervisor_relay.rs` | 🔴 red |
| `a_streamed_spawn_reports_started_each_stderr_line_and_its_exit_with_the_tail` | `packages/tddy-supervisor/tests/` | 🔴 red |
| `a_streamed_spawn_runs_in_a_cgroup_scope` (Linux only) | same | 🔴 red |
| `the_supervisor_names_no_coding_agent` (scans `src/`, `proto/`, `supervisor.yaml.production`, `dev.supervisor.yaml`) | `packages/tddy-supervisor/tests/` | 🔴 red |
| `a_supervised_clone_reports_progress_parsed_by_the_daemon` | `packages/tddy-daemon-rpc/tests/supervisor_clone_delegation.rs` | 🔴 red |
| `a_supervised_clone_is_moved_into_place_by_the_session_user` | same (Linux only for a real other user) | 🔴 red |
| `a_failed_supervised_clone_leaves_no_partial_directory` | same | guard (re-pointed) |
| `an_ssh_clone_needs_no_supervisor_env_configuration` | `packages/tddy-spawn/` (`supervisor_spawn` tests) | 🔴 red |
| `the_supervisors_opaque_refusal_names_the_exec_roots` (replaces :322) and `…_names_the_env_rule` (replaces :340) | `packages/tddy-session-lifecycle/src/coding_agent_relay.rs` (`#[cfg(test)]`) | 🔴 red |
| `install_migrates_the_old_spawn_policy_keys_once` | `packages/tddy-e2e/tests/install_supervisor.rs` | 🔴 red |
| `the_installed_supervisor_config_names_no_coding_agent` | same | 🔴 red |

## Responsibility

The supervisor's spawn surface, made generic:
- the exec gate (exec roots, fd exec);
- the one env rule;
- the streamed spawn that replaces `CloneRepository`, and the daemon's clone over it;
- cgroup scopes for every spawn kind;
- the migration away from the old keys;
- the guard that keeps agent names out.

## Boundaries

- Does **not** touch `users.yaml`, the per-user sockets, or the user part of the spawn policy (#859).
- Does **not** change `allowed_mount_roots` or the sandbox jail's construction.
- Does **not** change the daemon's agent env computation (#836).
- Does **not** add runtime-specific variables (`NODE_OPTIONS`, …) to the loader list.
- Does **not** make supervisor refusals descriptive to the caller. They stay opaque, and the reason is logged.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/env-allow-list` (#836, `#agent-tools` 28) — **behaviour** | `policy::resolve_relayed_env`, `EnvironmentBase::Exactly`; the daemon computes an agent's whole env | generalises the rule to every spawn kind; keeps `Exactly` for relay | change `agent_env`, `pass_env` or the base set |
| `feature/agent-tools/user-mapping-in-supervisor` (#859, `#agent-tools` 31) — **shared files** | `users.yaml`, session users from the mapping, one per-user socket, the old-key load errors, and `./install`'s one-time migration | edits the same files after it: `spawn_policy.rs`, `policy.rs`, `server.rs`, `config.rs` validation, both templates, the install migration, `install_supervisor.rs`; extends the migration with three more keys | re-implement the mapping, the sockets or the migration framework |

There is no edge with #857 (29) or #858 (30), which are line order only.

## Draft PR contract

The first push after this plan, with bodies marked `TODO(supervisor-agent-agnostic)`:
- `SpawnPolicy.exec_roots` and the old-key errors;
- `policy::resolve_exec`;
- the single env-rule function;
- the `SpawnStreamed` proto messages and RPC (generated code included);
- `SpawnRelayedRequest.scope`;
- the daemon-side clone signature over `SpawnStreamed`;
- the failing tests above.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. The env-rule tests generalise #836's relayed-env behaviour, and the config and migration tests edit what #859's green writes.
**Concurrent with:** none (#859 must be green first)
**Blocks:** nothing

Real dependency edges:

    #836 → this node      #859 ⇢ this node (shared files and migration)

## Decisions & Trade-offs

Decided by the user, 2026-11-07, via orchestrator AskUserQuestion:

- **Exec gate: "Trusted exec roots (Recommended)".**
  - One `exec_roots: [__INSTALL_BIN_DIR__]` replaces both lists.
  - The trust check covers the file and every parent.
  - On Linux the checked fd is exec'd.
  - Relaying is a stdio mode.
  - Harness binaries are named only in the daemon's spec YAML.
- **Env: "Yes, one generic rule (Recommended)".**
  - `allowed_env_keys` and `CloneSshEnv::ALLOWED_ENV_KEYS` are deleted, and the daemon's env passes whole.
  - Loader keys are refused, `HOME`/`USER`/`LOGNAME` forced, `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` stripped.
  - Base is `Minimal` for session/sandbox/streamed and `Exactly` for relay.
  - No runtime-specific loader names.
- **Clone: "Generic spawn in 32".**
  - `SpawnStreamed` replaces `CloneRepository`.
  - The daemon composes `git clone`, parses progress (`git_progress.rs`) and renames as the session user, which removes the root rename in a user-writable directory.
- **cgroups: "Include in 32 (Recommended)".**

Proposed defaults, **not yet confirmed by the user. Confirm at pass 2:**

1. The default exec root is only the install bin dir.
2. A symlink is judged by its target, which must itself lie under a root.
3. macOS dev does a path-only check; fd exec is Linux-only, and the tests say so.
4. The old keys are load errors naming the replacement, and `./install` migrates them once (extending #859's migration).
5. The guard scans `src/`, `proto/` and the templates. The `/usr/bin/claude` fixture (`tests/supervisor_call_deadline.rs:119`) is renamed.

Found while planning (pass 2 must ask):

- **git is not under the install bin dir.** Today `CloneRepository` runs git from the supervisor's
  `PATH` (`clone.rs:57-71`), and git must be in `allowed_tool_paths`. With default 1, a streamed
  `git clone` at `/usr/bin/git` is refused. The same holds for harnesses outside the bin dir:
  `opencode` and `claude-agent-acp` are documented at `/usr/local/bin/…`, and dev's `codex-acp` is
  in `node_modules/.bin`. The choices are:
  - (a) list `/usr/bin` as a root (see risk 2);
  - (b) have `./install` place or link a git under the bin dir (a symlink is judged by its target,
    so a link alone fails default 2);
  - (c) a second, narrow root.
- **"The env gate already admits PATH" is not quite right.** It is admitted only when an operator
  lists it: `spawn_broker.rs:150-155` says `PATH` is granted through `allowed_env_keys`, and the
  shipped lists do not include it. `GIT_SSH_COMMAND` *is* admitted on any host configured for SSH
  clones (`clone_env.rs:29-37`). The conclusion holds: the binary list is defence in depth.
- **A directory check with a sticky exception.** `require_trustworthy_config_directory`
  (`config.rs:417-447`) accepts a sticky world-writable directory. For an exec root, that would
  accept `/tmp/…` parents. The file-owner check still refuses a file another user dropped there,
  but pass 2 decides whether the exec gate keeps the sticky exception.

Risks:

1. **An old `supervisor.yaml` fails to load, and the supervisor runs as root.** A host upgraded
   without `./install` loses its supervisor and therefore its daemon, so the migration is
   mandatory.
2. **`/usr/bin` as a root silently means "no gate"** for practically every system binary.
3. **fd exec is Linux-only.** On macOS a swap between check and exec is not closed.
4. **Refusals show only at run time,** as a spawn failure; the reason is only in the supervisor's
   log.
5. **The proto change strands mixed builds.** A daemon and a supervisor from different builds
   cannot clone.

Complexity 5 is a warning that the boundary may be wrong. The seams would be:
- (i) exec roots;
- (ii) the env rule;
- (iii) `SpawnStreamed` with the daemon-side clone;
- (iv) cgroup scopes.

It is one node at the user's direction.

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
