# Changeset: The supervisor names no coding agent: exec roots, one env rule, a generic streamed spawn

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**PR**: [#865](https://github.com/uppin/tddy-coder/pull/865) — `#agent-tools` 32/32, base `feature/agent-tools/user-mapping-in-supervisor`
**Complexity**: 5. It spans three crates (supervisor, spawn, session-lifecycle), plus e2e tests and the install script. It changes the proto contract (`CloneRepository` → `SpawnStreamed`, plus scopes on relayed and streamed spawns), migrates config data, and changes a security gate (exec roots, fd exec). Pass 2 confirmed the defaults, and found that every existing spawning test fixture must move off `/tmp` once the gate is wired.

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
  - `clone_env.rs` (:1-37): `ALLOWED_ENV_KEYS` and the module doc go (and its test
    `tests/clone_env.rs:94`).
  - `supervisor_spawn.rs`: `plan_supervised_clone` (pass 2 surface) — the one `SpawnStreamed`
    request of a clone: `tddy-git-clone` as the session user.
- **tddy-git-clone** (new crate, binary `tddy-git-clone`): the clone helper. Its body is a TODO.
- **Scripts**: `./release`, `./install`, `scripts/dev-runtime-binaries.sh`, `./publish.sh` ship it.
  - `supervisor_spawn.rs`:
    - the module doc;
    - `clone_repo_via_supervisor_with_progress` (:250-262 and on) is rewritten over `SpawnStreamed`, with progress parsed by `git_progress.rs`;
    - the rename is done as the session user.
  - `supervisor_client.rs`: the client for the new RPC.
- **tddy-session-lifecycle**: `coding_agent_relay.rs`, whose refusal text (:9-11, :101-111) and tests (:322, :340) name the removed keys. #859 owns the `allowed_session_users` → `users.yaml` wording at :331.
- **tddy-daemon-rpc**: no test change at pass 2 (`tests/supervisor_clone_delegation.rs` keeps its
  unreachable-supervisor test; see Acceptance Tests).
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
- [ ] `SpawnStreamed` replaces `CloneRepository` (proto). A clone is one streamed spawn of the `tddy-git-clone` helper as the session user; the daemon parses its progress.
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

Written at pass 2 (2026-11-08). 🔴 = fails on this node's own `TODO(supervisor-agent-agnostic)`;
guard = passes on the stub and must keep passing. **Linux** = `#[cfg(target_os = "linux")]`, confirmed by
CI only. **#859** = also needs #859's session-user mapping green (see Dependencies).

| Test | File | Kind |
|---|---|---|
| `a_binary_under_an_exec_root_directory_owned_by_the_supervisor_is_run` | `packages/tddy-supervisor/src/policy.rs:1345` | 🔴 |
| `a_binary_an_exec_root_names_as_a_file_is_run_and_its_sibling_is_not` | `policy.rs:1359` | guard |
| `a_binary_outside_every_exec_root_is_refused` | `policy.rs:1378` | guard |
| `a_binary_whose_file_or_any_parent_is_writable_by_others_is_refused` — `#[rstest]` group/world-writable file, group/world-writable parent, sticky world-writable parent | `policy.rs:1398` | 🔴 ×5 |
| `a_binary_owned_by_another_user_is_refused` (the trust predicate; no root to chown with) | `policy.rs:1415` | 🔴 |
| `a_symlink_under_a_root_to_a_file_outside_every_root_is_refused` | `policy.rs:1435` | guard |
| `a_symlink_outside_every_root_to_a_file_a_root_names_runs_that_file` | `policy.rs:1452` | 🔴 |
| `a_file_swapped_after_the_check_is_not_what_is_execd` | `policy.rs:1470` | 🔴 Linux |
| `on_macos_the_exec_gate_is_path_only_and_execs_the_checked_path` | `policy.rs:1489` | 🔴 macOS |
| `the_spawn_env_rule_passes_the_daemons_env_whole_and_forces_the_account` — `#[rstest]` session, sandbox, streamed (`Minimal`), relayed (`Exactly`) | `packages/tddy-supervisor/src/spawn_broker.rs:2027` | 🔴 ×4 |
| `a_loader_variable_is_refused_for_every_spawn_kind` — `#[rstest]` ×4 | `spawn_broker.rs:2068` | guard |
| `an_old_spawn_policy_key_fails_loading_naming_its_replacement` — `#[rstest]` ×3 | `packages/tddy-supervisor/src/config.rs:851` | 🔴 ×3 |
| `an_exec_root_that_is_not_absolute_fails_loading` | `config.rs:863` | 🔴 |
| `reads_exec_roots_naming_directories_single_files_and_trusted_prefixes` | `config.rs` | guard (surface) |
| `a_streamed_spawn_of_a_binary_outside_every_exec_root_is_refused` | `packages/tddy-supervisor/tests/supervisor_streamed_spawn.rs:25` | guard |
| `a_streamed_spawn_reports_started_each_stderr_line_and_its_exit_with_the_tail` | `supervisor_streamed_spawn.rs:89` | 🔴 Linux, #859 |
| `a_streamed_spawn_runs_in_a_cgroup_scope` | `supervisor_streamed_spawn.rs:125` | 🔴 Linux, #859 |
| `a_relayed_spawn_runs_in_a_cgroup_scope` | `packages/tddy-supervisor/tests/supervisor_relay.rs:212` | 🔴 Linux, #859 |
| `the_supervisor_names_no_coding_agent` (names from the coding-agent spec files; non-test `src/`, `proto/`, both templates) | `packages/tddy-supervisor/tests/supervisor_names_no_coding_agent.rs:88` | 🔴 |
| `a_supervised_clone_is_one_streamed_spawn_of_the_clone_helper_as_the_session_user` | `packages/tddy-spawn/tests/supervised_clone_plan.rs` | 🔴 |
| `a_clone_goes_into_a_partial_sibling_and_is_renamed_into_place` | `packages/tddy-git-clone/tests/clone_helper.rs` | 🔴 |
| `a_failed_clone_removes_its_partial_directory_and_carries_gits_words` | `clone_helper.rs` | 🔴 |
| `a_destination_that_already_exists_is_refused_and_left_alone` | `clone_helper.rs` | 🔴 |
| `install_ships_the_clone_helper_beside_the_daemon`, `release_builds_the_clone_helper`, `publish_packages_the_clone_helper_into_the_deb`, `a_dev_run_built_with_the_build_flag_has_the_clone_helper` | `packages/tddy-e2e/tests/clone_helper_shipping.rs` | 🔴 ×4 |
| `a_binary_under_a_trusted_prefix_root_whose_own_directory_is_group_writable_and_sticky_runs` | `packages/tddy-supervisor/src/policy.rs` | 🔴 |
| `the_same_store_root_without_the_trusted_prefix_mark_is_refused` | `policy.rs` | guard |
| `a_group_writable_directory_below_a_trusted_prefix_is_still_refused` | `policy.rs` | guard |
| `an_ssh_clone_needs_no_supervisor_env_configuration` | `supervised_clone_plan.rs` | 🔴 |
| `the_supervisors_opaque_refusal_names_the_exec_roots` (replaces `…_names_the_relay_tool_allowlist`) | `packages/tddy-session-lifecycle/src/coding_agent_relay.rs:319` | 🔴 |
| `the_supervisors_opaque_refusal_names_the_env_rule` (replaces `…_names_the_environment_allowlist`) | `coding_agent_relay.rs:340` | 🔴 |
| `install_migrates_the_old_spawn_policy_keys_once` | `packages/tddy-e2e/tests/install_supervisor.rs:1215` | 🔴 |
| `the_installed_supervisor_config_names_no_coding_agent` | `install_supervisor.rs:1280` | 🔴 |
| `a_fresh_install_renders_exec_roots_naming_the_bin_dir_and_the_resolved_git` (plus `/nix/store` as a trusted prefix when git lives there) | `install_supervisor.rs` | 🔴 |

Line numbers are as written at pass 2 and move with green. Changed from pass 1: the daemon-side clone is pinned by **a pure plan test** in `tddy-spawn`, not by
end-to-end tests in `tddy-daemon-rpc/tests/supervisor_clone_delegation.rs`. A daemon-side test
against a real supervisor needs the `tddy-supervisor` binary from another crate, which no cheap seam
provides. The end-to-end streamed behaviour is pinned at the RPC level in `tddy-supervisor/tests/`.

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

Found at pass 2 (2026-11-08):
- **The env-rule tests do not depend on #836's body.** They drive this node's own
  `policy::resolve_spawn_env`, and their child-environment assertions (the daemon's keys arrive,
  `HOME`/`USER`/`LOGNAME` forced, `LISTEN_*` and `TDDY_SUPERVISOR_SOCKET` stripped) hold on #836's
  stubbed `EnvironmentBase::Exactly` too. What #836 alone pins — no supervisor `PATH` or locale under
  `Exactly` — stays in #836's `a_relayed_agent_starts_from_exactly_the_env_the_daemon_computed`.
- **Three Linux integration tests could not be separated from #859**:
  `a_streamed_spawn_reports_started_each_stderr_line_and_its_exit_with_the_tail`,
  `a_streamed_spawn_runs_in_a_cgroup_scope` and `a_relayed_spawn_runs_in_a_cgroup_scope`. Each
  spawns as the current user via the shared fixture's `allowing_the_current_user()`, which writes
  `allowed_session_users`; #859's green makes that key a load error and must move the shared fixture
  to `users.yaml`. They fail on this node's stubs today, but go green only after both.
- **Green must move the integration fixtures off `/tmp`.** On Linux every `ToolFixture` lives in a
  `/tmp` tempdir, and a sticky world-writable directory is not trusted, so every existing spawning
  test is refused once the exec gate is wired. `support::a_trusted_tool` (under
  `CARGO_TARGET_TMPDIR`) is the pattern; the old `allowing_tool`/`allowing_relay_tool` fixture keys
  become `with_exec_root`.

## Draft PR contract

Pushed at pass 2 (2026-11-08), with bodies marked `TODO(supervisor-agent-agnostic)`:
- `SpawnPolicy.exec_roots: Vec<ExecRoot>` (a path, or `{ path, trusted_prefix }`); the old-key load
  errors are a TODO in `config.rs` validation;
- `policy::resolve_exec` → `CheckedExecutable` (`path()`, `exec_path()`), and
  `policy::is_trusted_exec_path_entry`;
- the one env rule: `policy::resolve_spawn_env(SpawnKind, env) -> SpawnEnvironment`;
- `rpc SpawnStreamed` with `SpawnStreamedRequest` / `StreamedEvent` (`Started`, `Line`, `Exited`),
  `PrivilegedSurface::spawn_streamed` (stub: denied) and a real `SupervisorClient::spawn_streamed`;
- `SpawnRelayedRequest.scope` (wire field 7 and the Rust request);
- `tddy_spawn::supervisor_spawn::plan_supervised_clone` — one `SpawnStreamed` of `tddy-git-clone`;
- the `tddy-git-clone` crate (binary, TODO body);
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

Decided by the user at pass 2, 2026-11-08 (these replace pass 1's proposed defaults and settle what
pass 1 found):

1. **`exec_roots` entries may name individual trusted files** (`/usr/bin/git`) as well as
   directories. Each entry, file or directory, gets the owner and mode check. `./install` renders
   the bin dir plus the exact files git and the configured harnesses resolve to. The dev template
   documents the same, with the resolved target of each `node_modules/.bin` harness link. Tddy
   Desktop runs no supervisor, so there is nothing to render there. This settles "git is not under
   the install bin dir" as option (c), narrowed to single files.
2. **A symlink is judged by its target.**
3. **macOS does a path-only check.** There is no exec through a descriptor there.
4. **The old keys are load errors, and `./install` migrates them once** (extending #859's
   migration): listed tool and relay paths become `exec_roots` file entries; `allowed_env_keys` is
   dropped.
5. **A guard scan keeps coding-agent names out of `tddy-supervisor`'s non-test code** (and `proto/`
   and the two templates). The `/usr/bin/claude` fixture (`tests/supervisor_call_deadline.rs:119`) is
   renamed.
6. **A world-writable directory is refused even when sticky** (e.g. `/tmp`). The exec gate does not
   keep `require_trustworthy_config_directory`'s sticky exception.

Decided by the user at the 6e review, 2026-11-08:

7. **The acceptance tests and every green deletion listed at pass 2 are approved.**
8. **`./install` renders a live `spawn_policy`.** Session users come from `users.yaml` (#859), so
   nothing in it needs an operator before it can be live.
9. **`./install` renders the bin dir plus git only.** Harness files outside the bin dir are operator
   entries; rendering the configured harnesses is recorded in
   [`../todo/2026-11-08-install-renders-configured-harness-exec-roots.md`](../todo/2026-11-08-install-renders-configured-harness-exec-roots.md).
10. **The `tddy-spawn` plan test is the daemon-side seam** (no real-supervisor test in `tddy-daemon-rpc`).
11. **The guard bans spec-stem tokens**, not the phrase "coding agent".
12. **One node, complexity 5.**
13. **Trusted prefixes.** An `exec_roots` entry may be operator-marked `{ path: /nix/store,
    trusted_prefix: true }`: its own directory's owner and mode are not checked, everything below it
    still is. Only the operator opts in, per entry. `./install` renders `/nix/store` as a trusted
    prefix only when git resolves into it. (`/nix/store` is `drwxrwxr-t root:nixbld`, so without the
    mark every nix-store binary is refused.)
14. **The clone is ONE `SpawnStreamed` of a clone helper binary, `tddy-git-clone`** ("a special binary
    that runs on the OS user"). It lives in the bin dir beside the daemon and is spawned as the session
    user with `--git <git> -- <url> <destination>`. In one process it clones into a partial sibling,
    renames it into place, removes it on failure and refuses an existing destination, writing git's
    progress on stderr. No `mv`/`rm` spawns or `exec_roots` entries; the supervisor still knows nothing
    about clones. It is a new workspace crate with no external dependencies, and is shipped by
    `./release`, `./install` (`INSTALLED_BINARIES`), `scripts/dev-runtime-binaries.sh` and
    `./publish.sh`. It is not part of `./install --desktop`, since Desktop runs no supervisor. Green
    must add it to every install fixture's fake release binaries.

Also fixed in this node's contract commit: the stale `allowed_env_keys` notes that #836 left in
`supervisor.yaml.production` and `docs/ft/supervisor/tddy-supervisor.md` (a relayed agent's env
passes whole).

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-11-08)
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
