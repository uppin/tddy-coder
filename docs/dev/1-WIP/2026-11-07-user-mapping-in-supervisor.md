# Changeset: The GitHub → OS-user mapping lives in `users.yaml`, owned by whoever runs the deployment

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5 — several crates (supervisor, daemon-kernel, agent-launch, session-lifecycle, daemon, e2e), plus the install script and the YAML templates. It also carries a config-data migration, a supervisor→daemon contract change (the merged socket, the handover of mapping and paths), and restart ordering on a file watch.

## Initial Discovery

[2026-11-07-user-mapping-in-supervisor-initial-discovery.md](./2026-11-07-user-mapping-in-supervisor-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-01-install-script-over-length-budget.md](../todo/2026-11-01-install-script-over-length-budget.md) | **During.** This node deletes `survey_agent_tool_sockets` and `declare_agent_tool_sockets` (`install:716-902`, about 190 lines) and adds the one-time migration. Measure at wrap and narrow the record; the account/identity split it asks for stays open. |
| [2026-10-08-keyring-supervisor-host-socket-real-run.md](../todo/2026-10-08-keyring-supervisor-host-socket-real-run.md) | **Reference.** The merged socket goes through the same root `chown` and handover, which were never run under a real root supervisor. This node does not close that record; its Linux-only tests (`tests/supervisor_socket_handoff.rs`) must be re-pointed at the merged socket. |
| [2026-09-18-desktop-install-configures-no-identity.md](../todo/2026-09-18-desktop-install-configures-no-identity.md) | **Reference.** Desktop enrolment now writes `~/.tddy/users.yaml`; the record's owner verification is unchanged. |
| [`tddy-supervisor/docs/code-issues/oversized-file-supervisor.md`](../../../packages/tddy-supervisor/docs/code-issues/oversized-file-supervisor.md) | **Closed already** (379 lines after #721). `supervisor.rs` gains the watch and rebuild, so keep it under budget: those go in a module of their own. |

## Affected Packages

- **tddy-supervisor**:
  - **`users.yaml` loader.** Rows `{github_user, os_user}`, reusing the shape of `UserMapping` (`tddy-daemon-kernel/src/config/access.rs:27-30`). It applies the trust check `SupervisorConfig::load` uses (`config.rs:246-262`), refuses `root`, and allows at most 15 OS users.
  - **One per-user socket.** It replaces `services[].host_sockets` and `services[].agent_tool_sockets` (`config.rs:90-105`) and their validation (`host_socket_config.rs:16-57`). There is one deterministic path and one `LISTEN_FDNAMES` prefix per user.
  - `SocketHandover` (`handover.rs:26-61`): `with_host_session_socket` and `with_agent_tool_socket` become one method for the per-user socket. The mapping and the socket paths reach the daemon at exec.
  - `SpawnPolicy.allowed_session_users` (`config/spawn_policy.rs:7-10`) is removed. `policy::resolve_session_user` (`policy.rs:74-90`) reads the mapping's OS users.
  - **A watch on `users.yaml`.** It rebuilds the listeners, keeping unchanged users' descriptors, then restarts `tddy-daemon`. The bind loop at `supervisor.rs:116-124` and the restart path in `supervisor/service_restart.rs` are reused.
  - `socket.rs:24-38` (two prefixes) becomes one prefix.
  - The old keys fail loading, naming `users.yaml`.
- **tddy-daemon-kernel**:
  - `DaemonConfig.users: LiveUsers` (`config.rs:89-93`) loads from `users.yaml` when unsupervised and from the handover when supervised. A `users:` key in `daemon.yaml` fails loading, naming `users.yaml`.
  - The agent-tool socket (`agent_tool_sockets.rs`) becomes the per-user daemon socket:
    - the path is derived only when unsupervised (`agent_tool_socket_path_for_user`, :55-77);
    - its prefix (:40) becomes the one shared prefix.
  - `first_login_enrolment.rs` and `LiveUsers::enrol_first_login` (`live_users.rs:101`) write `users.yaml`.
- **tddy-agent-launch**:
  - The host-session socket (`host_session_socket.rs`, `<data dir>/run/<os user>/host.sock`) and `inherited_host_sockets.rs` are served on the merged per-user socket.
  - `agent_tool_socket_of` (`cli_agent_steps.rs:283-288`) answers the merged path.
- **tddy-session-lifecycle**:
  - The adoption at build.
  - The cross-crate prefix pin test (`connection_service/inherited_host_sockets_tests.rs:285-313`) becomes one prefix.
- **tddy-daemon**:
  - The adoption in `runtime/mount_host.rs:261,313` becomes one.
  - `UpdateConfig` (`daemon_config_service.rs:195-228`) stops re-serialising `users:` into `daemon.yaml` (:210-216). It writes `users.yaml` on desktop and never when supervised.
- **tddy-desktop**: desktop enrolment writes `~/.tddy/users.yaml`. Only touched if its config path code names `users:`.
- **tddy-e2e**: `tests/install_supervisor.rs` (:319-374, the agent-tool-socket install tests) and `tests/install_script.rs`.
- **Non-Rust**:
  - `install`: delete :716-902 and the call at :1461; add the one-time migration.
  - `supervisor.yaml.production` (:66-82, :98-99).
  - `daemon.yaml.production:159`, `desktop.yaml.production:104`, `dev.daemon.yaml:95`, `dev.desktop.yaml:95`.
  - `web-dev:176-180` (renders `CURRENT_USER`) and `desktop-dev`.

## Related Feature Documentation

- PRD: [PRD-2026-11-07-user-mapping-in-supervisor.md](../../ft/daemon/1-WIP/PRD-2026-11-07-user-mapping-in-supervisor.md)

## Summary

`users.yaml` is the one place a GitHub login is mapped to an OS user. On a supervised host, the
root supervisor reads it, derives one per-user daemon socket from it, hands the daemon the mapping
and the sockets at exec, and restarts the daemon when the file changes. On an unsupervised host the
daemon reads it itself. Three hand-kept supervisor lists and the install survey that synced them
are gone.

## Scope

- [ ] `users.yaml`: format, trust check, at most 15 users, `root` refused; supervised and unsupervised loaders
- [ ] One per-user daemon socket at a deterministic path: host-session services and the agent-tool roster merged; one `LISTEN_FDNAMES` prefix
- [ ] Handover at exec: the mapping, each socket path, the listeners. A supervised daemon derives no socket path.
- [ ] Spawn policy session users come from the mapping. `allowed_session_users` is removed.
- [ ] Watch `users.yaml`: rebuild listeners (unchanged users keep their descriptors), restart `tddy-daemon`; an invalid edit is refused and the running mapping kept
- [ ] Old keys are load errors naming `users.yaml`: `daemon.yaml` `users:`; `supervisor.yaml` `allowed_session_users`, `host_sockets`, `agent_tool_sockets`
- [ ] Desktop: `UpdateConfig` and first-login enrolment write `users.yaml`. A supervised daemon never writes the mapping.
- [ ] `./install`: one-time migration (`daemon.yaml` `users:` → `users.yaml`, old keys dropped, idempotent); `survey_agent_tool_sockets` and `declare_agent_tool_sockets` deleted
- [ ] Templates and dev scripts: `*.yaml.production`, `dev.*.yaml`, `./web-dev`, `./desktop-dev`
- [ ] Feature docs: `tddy-supervisor.md`, `systemd-install.md`, `session-auth.md`, `daemon-settings.md`

## Technical Changes

### State A

Read at #858's base, #836's tip `ddef5cc6b`. #834's code, the per-user agent-tool sockets, is
implemented there.

- The mapping is `DaemonConfig.users: LiveUsers` (`tddy-daemon-kernel/src/config.rs:89-93`,
  `live_users.rs`). About 60 call sites read it through `DaemonConfig::os_user_for_github`
  (`config/daemon_methods.rs:65`, `live_users.rs:66`).
- The supervisor knows users only by hand-kept lists:
  - `spawn_policy.allowed_session_users` (`config/spawn_policy.rs:7-10`, read by `policy.rs:81-84`);
  - `services[].host_sockets`, the agent-tool sockets' list `services[].agent_tool_sockets`, and
    the `HostSocket` type (`config.rs:90-124`).

  `host_socket_config.rs:34-48` caps `1 + host + agent_tool ≤ 16`. With two sockets per user,
  that is 7 users.
- The handover is exec-only (`handover.rs:26-61`):
  - descriptor 3 is `connection`;
  - each user then gets `host-session.<user>` and `agent-tools.<user>` (`socket.rs:16-38`);
  - `LISTEN_FDS` and `LISTEN_FDNAMES` announce them.

  The listeners are held across a service restart (`handover.rs:21-25`). There is no
  supervisor→daemon channel after exec, and nothing reloads (no SIGHUP, no file watch).
- The daemon has two per-user sockets:
  - host-session at `<data dir>/run/<user>/host.sock` (`tddy-agent-launch/src/host_session_socket.rs`);
  - agent-tool at `<data dir>/run-agent-tools/<user>/agent.sock` (`agent_tool_sockets.rs:55-77`).

  When supervised, it serves the handed listeners. It reads each one's path from the listener
  (`owner_socket.rs:312-315`) and refuses an unlisted user (`cli_agent_steps.rs:279-288`).
- `./install` declares `agent_tool_sockets:` by surveying `supervisor.yaml` with `awk`
  (`install:716-902`, called at :1461). `users:` appears at `dev.daemon.yaml:95`,
  `dev.desktop.yaml:95`, `daemon.yaml.production:159` and `desktop.yaml.production:104`.
- `UpdateConfig` re-serialises the live `users:` rows into `daemon.yaml`
  (`daemon_config_service.rs:210-216`). Enrolment writes the first row there
  (`first_login_enrolment.rs:62`).

### State B

See the PRD's "What's Changing". In short:

- `users.yaml` is the one source of the mapping.
- The supervised daemon gets the mapping and one socket per user from the supervisor.
- An unsupervised daemon reads `users.yaml` and derives its own socket paths.
- A mapping change restarts the daemon under the supervisor.

## Testing Plan

- Unit tests in `tddy-supervisor` for the loader, the socket derivation, the handover names and
  paths, the spawn policy, the old-key errors and the listener rebuild. Run with the current user
  standing in, as `host_socket.rs:241-294` does.
- Unit and integration tests in `tddy-daemon-kernel` and `tddy-daemon` for loading, enrolment and
  `UpdateConfig`.
- `tddy-e2e` install tests for the migration (`INSTALL_NO_SYSTEMCTL=1`, as today).
- Real-root handover stays Linux-only, like `tests/supervisor_socket_handoff.rs`.

## Acceptance Tests

| Test | File | Kind |
|---|---|---|
| `a_mapped_os_user_gets_one_daemon_socket_at_its_derived_path` | `packages/tddy-supervisor/src/` (`#[cfg(test)]`, module chosen in pass 2) | 🔴 red |
| `the_daemon_is_handed_the_mapping_and_every_user_socket_path_at_exec` | same | 🔴 red |
| `a_sixteenth_mapped_user_fails_loading_naming_users_yaml_and_the_limit` | same | 🔴 red |
| `a_session_may_run_only_as_an_os_user_the_mapping_names` | `packages/tddy-supervisor/src/policy.rs` (`#[cfg(test)]`) | 🔴 red |
| `a_users_yaml_another_user_could_write_is_refused` | `packages/tddy-supervisor/src/` | 🔴 red |
| `an_old_user_list_key_in_supervisor_yaml_fails_loading_naming_users_yaml` | `packages/tddy-supervisor/src/config.rs` (`#[cfg(test)]`) | 🔴 red |
| `a_changed_users_yaml_rebuilds_the_listeners_keeping_unchanged_descriptors_and_restarts_the_daemon` | `packages/tddy-supervisor/tests/` | 🔴 red |
| `an_invalid_users_yaml_edit_keeps_the_running_mapping` | same | 🔴 red |
| `a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket` | `packages/tddy-session-lifecycle/` (beside `inherited_host_sockets_tests.rs`) | 🔴 red |
| `a_supervised_daemon_derives_no_socket_path_of_its_own` | same | 🔴 red |
| `the_supervisor_and_the_daemon_share_one_user_socket_prefix` (replaces the two-prefix pin, :285-313) | same | 🔴 red |
| `a_users_key_in_daemon_yaml_fails_loading_naming_users_yaml` | `packages/tddy-daemon-kernel/src/config.rs` (`#[cfg(test)]`) | 🔴 red |
| `first_login_enrolment_on_desktop_writes_users_yaml_and_no_daemon_yaml_users_key` | `packages/tddy-daemon-kernel/src/first_login_enrolment.rs` tests | 🔴 red |
| `update_config_writes_no_users_key_into_daemon_yaml` | `packages/tddy-daemon/` (`daemon_config_service` tests) | 🔴 red |
| `a_supervised_daemon_refuses_to_enrol` | `packages/tddy-daemon-kernel/` | 🔴 red |
| `install_moves_daemon_yaml_users_into_users_yaml_once_and_drops_the_old_keys` | `packages/tddy-e2e/tests/install_supervisor.rs` | 🔴 red |
| `install_declares_no_per_user_sockets_in_supervisor_yaml` (replaces :374) | same | 🔴 red |

## Responsibility

- Where the GitHub → OS-user mapping lives.
- Who reads it in each deployment.
- The per-user daemon socket set derived from it.
- The supervisor→daemon handover of the mapping and the sockets.
- Restarting on a mapping change.
- The one-time install migration.

## Boundaries

- Does **not** change the lookup (`os_user_for_github` semantics, no default arm) or its ~60 readers.
- Does **not** add a live supervisor→daemon channel; a mapping change is a restart.
- Does **not** keep relayed agents alive across that restart (see Decisions).
- Does **not** remove the supervisor's other agent- or env-specific config:
  - `allowed_relay_tool_paths`;
  - `allowed_env_keys` for `SpawnSession` and clone spawns;
  - the clone-SSH env list.

  Making the supervisor agent-agnostic is the planned successor **`supervisor-agent-agnostic`
  (`#agent-tools` 32)**, discovered separately.
- Does **not** add the second-account enrolment path (`#keyring` 8/9's).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/agent-tools/agent-tool-socket-per-user` (#834, `#agent-tools` 26) | per-OS-user agent-tool sockets: the supervisor's `agent_tool_sockets:`, `SocketHandover::with_agent_tool_socket`, `AGENT_TOOLS_FD_NAME_PREFIX`, `AgentToolSockets` and its adoption, and install's `declare_agent_tool_sockets` | merges them with the host-session socket into one per-user socket; deletes the hand-kept list and the install survey | change what the agent-tool roster serves, or the hooks' fail-quiet contract |
| `feature/agent-tools/env-allow-list` (#836, `#agent-tools` 28) | `policy::resolve_relayed_env` and `EnvironmentBase::Exactly` in `tddy-supervisor` | edits the same crate after it (`policy.rs`, `spawn_broker.rs`); only a file-overlap edge | touch relayed env handling |
| `feature/agent-tools/operator-start-refusal` (#858, `#agent-tools` 30) | nothing; it is the branch base only | — | — |

## Draft PR contract

The first push after this plan, with bodies marked `TODO(user-mapping-in-supervisor)`:

- the owned surface:
  - the `users.yaml` type and loader signatures (supervisor and daemon-kernel);
  - the merged per-user socket's path function and its fd-name prefix;
  - the handover method that replaces the two;
  - the old-key errors;
  - the watch-and-restart entry point;
- the failing tests above.

The install migration's test is red against today's script.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. Its socket tests need #834's agent-tool socket *behaviour* (adoption, serving the roster) in order to merge it, and #834's green must have landed first. #836 is a file-overlap edge, not a behaviour edge.
**Concurrent with:** `feature/agent-tools/opencode-route-inline` (#857)
**Blocks:** `supervisor-agent-agnostic` (`#agent-tools` 32, not yet opened)

Real dependency edges:

    #834 → this node      #836 ⇢ this node (shared supervisor files)      this node → #agent-tools 32

## Decisions & Trade-offs

All of the following were decided by the user on 2026-11-07:

- `users.yaml` is a separate file, read by whoever owns the deployment.
- The supervisor derives one generic per-user socket. That merges the two kinds, lifts the
  7-user cap to 15, and leaves the daemon deriving no path when supervised.
- On a change, the supervisor re-reads the file and restarts the daemon.
- The three supervisor lists and the install survey are removed.
- `./install` migrates once; after that, an old key is an error.
- On desktop, `UpdateConfig` and enrolment write `users.yaml`.
- The supervisor rule (verbatim): "supervisor shouldn't be aware of any env or coding agent
  specifics. It's job is to manage tddy-daemon on the correct OS user, manage cgroups etc. It
  shouldn't inherit config that normally should belong to tddy-daemon. Also, any coding specific
  stuff should be configurable by YAML, so also allow-lists." The mapping is identity and
  authorization, not agent-specific, so it belongs here.

Risks:

- **A restart kills relayed agents and in-memory state.**
  - The relays go, because the daemon is the relay's other end.
  - So do open vaults, room connections, inference subscriptions, admissions and hosted clones.
  - Sessions survive on disk and resume.

  Adding a user is therefore disruptive. A live channel was rejected as out of scope.
- **Desktop has no supervisor.** The daemon owns `users.yaml` there, so two loaders exist (the
  supervised handover and the unsupervised file). Both must refuse the old keys the same way.
- **The cross-crate fd-prefix pin test** (`inherited_host_sockets_tests.rs:285-313`) links
  `tddy-supervisor` into `tddy-session-lifecycle`'s tests. The merge changes both prefixes in one
  PR, so the pin must move with them, or a half-merged state compiles and silently stops adopting.
- **Trust.** `users.yaml` grants OS users to GitHub logins, and the root broker enforces it. It
  needs the same owner, mode and parent-directory check as `supervisor.yaml` (`config.rs:240-262`),
  and a supervised daemon (an unprivileged account) must never be able to write it.

Open for pass 2, each to be asked rather than guessed:

1. **How the mapping reaches the daemon at exec:**
   - (a) an environment variable carrying the rows. This conflicts with #836's rule that `TDDY_*`
     never reaches an agent, so it would need clearing on adoption the way `LISTEN_*` is.
   - (b) an inherited read-only descriptor (a sealed memfd or pipe) beside the listeners.
   - (c) the path of the root-owned file, mode `0644`.
2. **Whether the paths are needed.** The handed listener already carries its path
   (`owner_socket.rs:312-315` reads it with `local_addr`). Handing each path again is redundant
   unless the daemon must know a user's path before that user's listener is adopted.
3. **The file watch.** `notify` would be a new external dependency, which needs asking first
   (CLAUDE.md). The alternatives are an `mtime` poll or a `SIGHUP` that `./install` and operators
   send.
4. **Where the merged socket lives**, for example `/run/tddy/users/<os user>/daemon.sock` with
   prefix `user.`. Every directory above it must be root-only (`host_socket.rs:14-19`), and the
   path must be short enough for `AF_UNIX`.
5. **Whether the supervisor should read only the `os_user` column.** It would hand the file to
   the daemon rather than parse GitHub logins it never uses, which keeps it further from daemon
   config.

Complexity 5 is a warning that the boundary may be wrong. The natural split is:

- (i) the merged per-user socket and the 15-user cap;
- (ii) `users.yaml`, the handover and the migration;
- (iii) the watch and restart.

It was kept as one node at the user's direction.

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
