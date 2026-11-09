# Changeset: The GitHub → OS-user mapping lives in `users.yaml`, owned by whoever runs the deployment

**Date**: 2026-11-07
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 5 (re-scored in pass 2, still the top of the scale and heavier than planned) — several crates (supervisor, daemon-kernel, daemon-auth, agent-launch, session-lifecycle, daemon, e2e) plus test-fixture churn across five more (daemon-livekit, host-service, daemon-auth, model-registry, daemon). It also changes `./install` (all three modes), `./web-dev` and `./desktop-dev`, the YAML templates, a config-data migration, the supervisor→daemon contract (merged socket, `TDDY_USERS_FILE`), a new dependency (`notify`) and restart ordering on a file watch. The natural split, if the user wants one, is unchanged (see Decisions).

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
  - `UpdateConfig` (`daemon_config_service.rs:195-228`) stops re-serialising `users:` into `daemon.yaml` (:210-216). It writes no `users.yaml` (user decision on the pass-2 report: its wire message carries no mapping; first-login enrolment is the one writer of `users.yaml`).
- **tddy-desktop**: desktop enrolment writes `~/.tddy/users.yaml`. Only touched if its config path code names `users:`.
- **tddy-daemon-auth**: `first_login_admission.rs` handles the new `EnrolmentRefusal::OwnedBySupervisor`; its tests that load `users:` from a file move to `users.yaml`.
- **Fixture churn (user decision, 2026-11-07)**: about 25 test files load a `users:` key from a file through `DaemonConfig::load`, and that key is now a load error. They are in `tddy-daemon-livekit` (4 repro/acceptance suites), `tddy-host-service` (`test_util.rs` and 5 handler unit-test files), `tddy-daemon-auth` (7 acceptance suites and `support/mod.rs`), `tddy-model-registry` (`model_registry_service_acceptance.rs`) and `tddy-daemon` (multi-host, staging-forwarding, session-agent-remote, removed-agent-config, first-login and more). Each one either writes `users.yaml` beside the config or sets `config.users` in memory after loading.
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
- [ ] Desktop: `UpdateConfig` writes no `users.yaml` (its wire message carries no mapping); first-login enrolment is the one writer of `users.yaml`. A supervised daemon never writes the mapping.
- [ ] `./install`: one-time migration (`daemon.yaml` `users:` → `users.yaml`, old keys dropped, idempotent); `survey_agent_tool_sockets` and `declare_agent_tool_sockets` deleted
- [ ] Templates and dev scripts: `*.yaml.production`, `dev.*.yaml`, `./web-dev`, `./desktop-dev`. A missing `users.yaml` is an error, so `./install` (`--systemd`, `--user`, `--desktop`), `./web-dev` and `./desktop-dev` each render one. `./web-dev` renders the daemon config into a `mktemp` file, so it needs a `mktemp -d` directory to put `users.yaml` beside it.
- [ ] Test fixture churn across `tddy-daemon-livekit`, `tddy-host-service`, `tddy-daemon-auth`, `tddy-model-registry` and `tddy-daemon` (see Affected Packages)
- [ ] The supervisor's integration fixture (`tests/support/mod.rs`) always emits `spawn_policy.allowed_session_users`; it moves to `users.yaml`
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

Written in pass 2 (2026-11-08). Each is red because this node's surface is a
`TODO(user-mapping-in-supervisor)` stub, unless marked otherwise. The two supervisor integration
tests are `#![cfg(target_os = "linux")]`: they compile out on macOS, so CI verifies them.

| Test | File | Validates | State |
|---|---|---|---|
| `a_mapped_os_user_gets_one_daemon_socket_at_its_derived_path` | `packages/tddy-supervisor/src/user_sockets.rs:103` | one socket per OS user at `<sockets_dir>/<u>/daemon.sock`, accepting | 🔴 |
| `a_rebuild_keeps_the_listener_of_a_user_the_new_mapping_still_names` | `packages/tddy-supervisor/src/user_sockets.rs:130` | unchanged user keeps the same listener (`Arc::ptr_eq`) | 🔴 |
| `a_rebuild_drops_the_listener_of_a_user_no_longer_mapped` | `packages/tddy-supervisor/src/user_sockets.rs:156` | an unmapped user loses the socket | 🔴 |
| `the_daemon_is_handed_the_users_file_and_one_named_listener_per_user_at_exec` | `packages/tddy-supervisor/src/handover.rs:186` | `connection:user.alice:user.bob` and the `users.yaml` path | 🔴 |
| `a_sixteenth_mapped_user_fails_loading_naming_users_yaml_and_the_limit` | `packages/tddy-supervisor/src/users_file.rs:132` | 15-user cap, named | 🔴 |
| `fifteen_mapped_os_users_fit` | `packages/tddy-supervisor/src/users_file.rs:118` | the cap is 15, not lower | 🔴 |
| `a_users_yaml_another_user_could_write_is_refused` | `packages/tddy-supervisor/src/users_file.rs:180` | trust check (mode) | 🔴 |
| `a_session_may_run_only_as_an_os_user_the_mapping_names` | `packages/tddy-supervisor/src/policy.rs:388` | spawn policy from the mapping | 🔴 |
| `an_old_user_list_key_in_supervisor_yaml_fails_loading_naming_users_yaml` (3 cases) | `packages/tddy-supervisor/src/config.rs:834` | `allowed_session_users`, `host_sockets`, `agent_tool_sockets` refused | 🔴 |
| `a_service_that_serves_users_needs_the_users_block` | `packages/tddy-supervisor/src/config.rs:869` | `serves_users` needs `users:` | 🔴 |
| `a_saved_edit_becomes_the_current_mapping_and_is_reported` | `packages/tddy-supervisor/src/users_watch.rs:143` | the watch reports a valid edit (10 s deadline) | 🔴 |
| `an_invalid_users_yaml_edit_keeps_the_running_mapping` | `packages/tddy-supervisor/src/users_watch.rs:160` | an invalid edit is refused; the mapping is kept | 🔴 |
| `the_daemon_is_handed_the_users_file_and_one_socket_per_mapped_user_at_exec` | `packages/tddy-supervisor/tests/supervisor_user_sockets.rs:19` | real supervisor: `LISTEN_FDNAMES`, `TDDY_USERS_FILE`, socket at its derived path | 🔴 Linux (CI) |
| `a_changed_users_yaml_rebuilds_the_listeners_keeping_unchanged_descriptors_and_restarts_the_daemon` | `packages/tddy-supervisor/tests/supervisor_user_sockets.rs:53` | real supervisor: a save restarts the daemon; fd 4's inode is unchanged | 🔴 Linux (CI) |
| `a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket` | `packages/tddy-session-lifecycle/src/connection_service/inherited_host_sockets_tests.rs:374` | one socket answers `github_token` and a roster call | 🔴 |
| `a_supervised_host_session_refusal_names_users_yaml` | same file `:440` | host-session refusal names `users.yaml`, not `host_sockets` | 🔴 |
| `the_supervisor_and_the_daemon_share_one_user_socket_prefix` | same file `:465` | cross-crate pin of `user.` and `TDDY_USERS_FILE` (replaces the two-prefix pin) | 🔴 |
| `a_supervised_daemon_derives_no_socket_path_of_its_own` | `packages/tddy-daemon-kernel/src/agent_tool_sockets_tests.rs:446` | `Supervised` placement binds nothing and names `users.yaml` | 🔴 |
| `a_users_key_in_daemon_yaml_fails_loading_naming_users_yaml` | `packages/tddy-daemon-kernel/src/config.rs:921` | `users:` is a load error | 🔴 |
| `an_unsupervised_daemon_reads_users_yaml_beside_its_config` | `packages/tddy-daemon-kernel/src/users_file.rs:113` | desktop/dev source | 🔴 |
| `a_supervised_daemon_reads_the_users_yaml_it_was_handed` | `packages/tddy-daemon-kernel/src/users_file.rs:135` | `TDDY_USERS_FILE` wins | 🔴 |
| `a_supervised_daemon_refuses_a_users_yaml_root_does_not_own` | `packages/tddy-daemon-kernel/src/users_file.rs:151` | root-only check when supervised (skipped as root) | 🔴 |
| `a_missing_users_yaml_fails_loading_naming_it` | `packages/tddy-daemon-kernel/src/users_file.rs:174` | no fallback to an empty mapping | 🔴 |
| `a_supervised_daemon_refuses_to_enrol` | `packages/tddy-daemon-kernel/src/live_users.rs:207` | `OwnedBySupervisor` | 🔴 |
| `first_login_enrolment_on_desktop_writes_users_yaml_and_no_daemon_yaml_users_key` | `packages/tddy-daemon-kernel/tests/first_login_enrolment_acceptance.rs:85` | enrolment writes `users.yaml`; `daemon.yaml` is byte-identical | 🔴 |
| `the_first_login_on_an_unenrolled_deployment_is_written_down` (rewritten) | same file `:66` | the row persists to `users.yaml` | 🔴 |
| `an_enrolled_deployment_refuses_to_enrol_a_second_account` (rewritten) | same file `:104` | once per deployment, via `users.yaml` | 🔴 |
| `an_unmapped_login_still_resolves_to_nobody` (rewritten) | same file `:132` | no default arm, via `users.yaml` | 🔴 |
| `update_config_writes_no_users_key_into_daemon_yaml` | `packages/tddy-daemon/tests/daemon_config_service.rs:297` | `UpdateConfig` stops writing `users:` | 🔴 |
| `install_moves_daemon_yaml_users_into_users_yaml_once_and_drops_the_old_keys` | `packages/tddy-e2e/tests/install_supervisor.rs:1072` | one-time migration, idempotent | 🔴 |
| `install_declares_no_per_user_sockets_in_supervisor_yaml` | `packages/tddy-e2e/tests/install_supervisor.rs:1139` | fresh install: `users:` block, `serves_users`, empty `users.yaml` at `0644` | 🔴 |

Unit tests (6f):

| Test | File | Validates | State |
|---|---|---|---|
| `names_a_users_one_daemon_socket_by_the_generic_prefix` | `packages/tddy-supervisor/src/socket.rs:158` | `user.<u>` | 🔴 |
| `reads_each_row_and_the_distinct_os_users_it_names` | `packages/tddy-supervisor/src/users_file.rs:100` | parse plus distinct OS users | 🔴 |
| `a_login_mapped_to_two_os_users_is_refused` | `packages/tddy-supervisor/src/users_file.rs:148` | an ambiguous login is refused | 🔴 |
| `a_mapping_to_root_is_refused` | `packages/tddy-supervisor/src/users_file.rs:162` | no `root` | 🔴 |
| `an_os_user_that_cannot_name_a_socket_is_refused` | `packages/tddy-supervisor/src/users_file.rs:171` | path and fd-name safety | 🔴 |
| `loads_a_users_yaml_only_the_supervisor_can_write` | `packages/tddy-supervisor/src/users_file.rs:200` | the trusted file loads | 🔴 |
| `a_session_as_root_is_refused_whatever_the_mapping_says` | `packages/tddy-supervisor/src/policy.rs:406` | guard | ✅ passes on the stub (guard) |
| `reads_where_the_user_mapping_and_its_sockets_live` | `packages/tddy-supervisor/src/config.rs:842` | `users:` block, default `sockets_dir` | ✅ (surface only) |
| `rejects_a_relative_users_file` | `packages/tddy-supervisor/src/config.rs:860` | absolute path | 🔴 |
| `an_unsupervised_daemon_derives_one_socket_per_user_under_its_run_dir` | `packages/tddy-daemon-kernel/src/agent_tool_sockets_tests.rs:416` | `<data>/run/<u>/daemon.sock` | 🔴 |
| `reads_each_user_socket_among_the_handed_over_ones` | `packages/tddy-daemon-kernel/src/agent_tool_sockets_tests.rs:431` | `user.` reader | 🔴 |
| `the_running_mapping_is_never_written_into_daemon_yaml` | `packages/tddy-daemon-kernel/src/config.rs:905` | `users` is never serialised | 🔴 |

Existing tests green must adapt to the new semantics. They stay green on the contract because the
stub bodies are unchanged:
- `live_users.rs` `concurrent_first_logins_enrol_exactly_one`,
  `a_row_enrolled_through_one_clone_is_seen_by_every_other` and
  `a_login_whose_enrolment_cannot_be_persisted_is_not_applied`, which pass a `config.yaml` path;
- `tddy-daemon/tests/first_login_enrolment_acceptance.rs` `users_written_to`, which reads
  `daemon.yaml`;
- the four approved deletions in `install_supervisor.rs`;
- `inherited_host_sockets_tests.rs` `reads_the_same_agent_tool_descriptor_names_the_supervisor_writes`
  and `refuses_a_user_the_supervisor_gave_no_socket_and_says_where_to_declare_one`;
- `agent_tool_sockets_tests.rs` `an_unlisted_user_under_the_supervisor_is_refused_naming_the_setting`;
- the supervisor's `host_sockets` and `agent_tool_sockets` unit and integration tests.

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

Sequencing found in 6f: no test of this node drives a parent's `TODO(...)` surface. The merged-socket
test (`a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket`) drives
#834's adoption and roster serving, which is green on the base. The scoped baseline on `0262752e3`
(2026-11-08) was green apart from #836's two supervisor env reds
(`policy::tests::a_relayed_agent_env_passes_whole_with_no_supervisor_key_list`,
`spawn_broker::tests::a_relayed_agent_starts_from_exactly_the_env_the_daemon_computed`).

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
- On desktop, enrolment alone writes `users.yaml`; `UpdateConfig` writes none.
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

Resolved in pass 2 (user, 2026-11-07, relayed by the orchestrator):

1. **How the mapping reaches the daemon:** (c), its **path**. The supervisor sets
   `TDDY_USERS_FILE=<users.file>` on the service that declares `serves_users: true`, beside
   `LISTEN_*`; the daemon reads the rows itself. When supervised the daemon refuses a file that is
   not root-owned or that group or others could write (the supervisor applies its own
   `supervisor.yaml` trust check, which reads "root-owned" when it runs as root).
2. **Paths are not handed separately.** The daemon reads each socket's path from its inherited
   listener (`owner_socket.rs:312-315`), so the handover is the file path plus one listener per
   user.
3. **The watch uses `notify`** (approved as a new dependency). Green adds it to
   `tddy-supervisor/Cargo.toml`; the contract stubs do not need it yet.
4. **Where the merged socket lives:** `<users.sockets_dir>/<os user>/daemon.sock`, default
   `sockets_dir: /run/tddy-users`, `LISTEN_FDNAMES` entry `user.<os user>`. Unsupervised, the
   daemon binds `<data dir>/run/<os user>/daemon.sock`, replacing both `run/<u>/host.sock` and
   `run-agent-tools/<u>/agent.sock`.
5. **The supervisor parses the rows but uses only `os_user`.** It validates the file it watches
   (shape, at most 15 distinct OS users, no `root`) and never reads a GitHub login.

`supervisor.yaml` after this node:

```yaml
users:
  file: /etc/tddy/users.yaml       # required once any service serves users
  sockets_dir: /run/tddy-users     # <dir>/<os user>/daemon.sock
services:
  - name: tddy-daemon
    serves_users: true             # gets TDDY_USERS_FILE and one `user.<os user>` listener each
```

User decisions on the pass-2 report (2026-11-07, relayed by the orchestrator):

- **`TDDY_USERS_FILE` is an env var** on the daemon (the service declaring `serves_users`). No
  agent process ever sees a `TDDY_*` variable (#836's allow-list).
- **`serves_users: true`** is the per-service flag naming which service is handed the user sockets
  and the file's path.
- **A missing `users.yaml` is an error**, never an empty mapping. `./install` (`--systemd`,
  `--user`, `--desktop`), `./web-dev` and `./desktop-dev` render one, so all of them are in scope.
- **`UpdateConfig` writes no `users.yaml`**: its wire message carries no mapping, so it only stops
  writing `users:` into `daemon.yaml`. First-login enrolment is the one writer of `users.yaml`.
- **`AgentToolSocketPlacement::Supervised`**: a supervised daemon derives and binds no socket path,
  and refuses a user it was handed no socket for, naming `users.yaml`.
- **Fixture churn is in scope.** About 25 test files load a `users:` key from a file through
  `DaemonConfig::load`, in `tddy-daemon-livekit`, `tddy-host-service`, `tddy-daemon-auth`,
  `tddy-model-registry` and `tddy-daemon`. Green rewrites them to write `users.yaml` beside the
  config, or to set `config.users` in memory after loading.
- **Deleting four `tddy-e2e/tests/install_supervisor.rs` tests is approved**:
  `installs_a_supervisor_config_that_documents_the_per_user_agent_tool_sockets`,
  `an_upgrade_declares_a_live_agent_tool_socket_for_every_user_the_policy_allows_sessions_as`,
  `an_operators_agent_tool_sockets_block_is_kept_and_each_policy_user_it_leaves_out_is_named` and
  `a_fresh_install_declares_no_agent_tool_socket_while_the_policy_allows_no_session_user`.

User decisions on the 6e review (2026-11-07, relayed by the orchestrator):

- **A GitHub login mapped to two OS users is refused** when `users.yaml` loads
  (`a_login_mapped_to_two_os_users_is_refused`), by the supervisor's loader and the daemon's.
- **One node, complexity 5.** The split below was offered again and declined.

The choices below were raised in the pass-2 report and are all approved above:

- **`TDDY_USERS_FILE` is an env var**, not an argv flag. The supervisor already sets
  `TDDY_SUPERVISOR_SOCKET` and `LISTEN_*` on a managed service; a flag would make it know the
  daemon's CLI. Either way it is the supervisor→daemon contract, not agent env, so the standing
  rule holds. #836's allow-list keeps it off every agent.
- **`serves_users: true`** on a service, rather than "the service that declares `socket:`", names
  which service is handed the user sockets.
- **The `users` field stays on `DaemonConfig`** as the runtime holder, and stays deserializable
  for in-memory configs (about 100 test literals build one with `users:`). Only
  `DaemonConfig::load` refuses the key on disk, the way it refuses `supervisor:`, and fills the
  holder from `users.yaml`. It is never serialised again.
- **`UpdateConfig` writes no `users.yaml`.** Its wire message carries no mapping, so after this node
  it only stops writing `users:`; enrolment is the one writer on desktop.
- **The host-session service is served on the merged socket** by registering it as a per-user
  service into `AgentToolSockets` (`UserScopedService`), so one server per user answers both.
  `HostSessionSockets` stops binding sockets of its own (`serve_on`).

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
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
