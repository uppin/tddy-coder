# Changeset: A target session's shell tab runs on the target

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — several crates (terminal-rpc, changeset, cli-sessions, agent-launch, session-lifecycle, coder, session-web, plus test support in testing-commons), a non-main target client launch beside a main-only one, vault/credential wiring into the roster and the restore (re-scored at the contract: the non-main client spawn lives in cli-sessions, and the owner is mapped back from the OS user)
**Stack**: `#session-liveness` 6/8 — branch `feature/session-liveness/target-shell-tab`, base `feature/session-liveness/sandbox-shell-tab`
**PR**: #965 https://github.com/uppin/tddy-coder/pull/965

## Initial Discovery

[2026-11-21-target-shell-tab-initial-discovery.md](./2026-11-21-target-shell-tab-initial-discovery.md)

## Affected Packages

- **tddy-terminal-rpc**: `target` accepted around a shell
- **tddy-changeset**: `TARGET_KIND` and `LaunchNode::target()`
- **tddy-agent-launch**: `launch_tab_on_target` (non-main target client), `TabTargetShell` port (resolve the session's target shell with the owner's credentials), `target` restore kind
- **tddy-session-lifecycle**: roster start with the target shell; the target worktree (`repo_path`) as the tab's directory
- **tddy-session-web**: "On target" / "On this host" menu group
- **tddy-coder**: the participant roster refuses `target`
- **tddy-cli-sessions**: `spawn_rendered_client_tab` — the non-main sibling of `spawn_rendered_client_main`
- **tddy-testing-commons** (test support): the stand-in machine's login shell answers `-l`

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-shell-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md)

## Summary

A shell tab opened "On target" on an ssh or Coder target session runs `cd '<worktree>' && exec "${SHELL:-/bin/sh}" -l` on the target — the target user's own login shell, with the target login's own environment (not an agent's `env -i` launch) — through the session's remote shell, as a local tab terminal. A tab "On this host" of a target session runs in the session's directory on this host. Recorded `[target, shell]`, reopened on the target by Resume, dropped with its reason when the target cannot be reached.

## Responsibility

- `launch_tab_on_target(shell, worktree, terminal_id, …)` in `remote_agent_launch`: builds the tab's remote command `cd '<wt>' && exec "${SHELL:-/bin/sh}" -l` (`target_shell_tab_command`: the target's login shell with its own environment, **not** `target_login_exec`'s `env -i`; no pid file, no pointer, no startup window, never `stop_session`), reaches the target first (`cd '<wt>'` through the port, so an unreachable target fails the tab with the transport's words before anything is registered), runs it through `RemoteShell::interactive` in a non-main local PTY (a non-main sibling of `spawn_rendered_client_main`).
- `TabTargetShell` port: resolve a session's target shell from `.session.yaml` `target` with `shell_for_session_target`, the caller's **subject** (the GitHub user the session token resolves to) and `CoderCredentialSource`; a locked vault is `failed_precondition`.
- The subject is threaded to the port: `TerminalLaunch.subject` (set in `start_terminal_session`, where the token is already resolved) and `RosterRestore.subject` / `restore_roster_on_resume(os_user, subject, …)` (from the resume's session token) — developer ruling, n1–n4 being on master.
- A tab "On this host" of a target session runs in the session's directory on this host (developer ruling): its worktree exists only on the target.
- Wire: `target` accepted around `bash`; `TerminalKind` variant; participant refusal arm.
- Recording `[target, shell]`; restore kind `target` → shell; drop with reason on an unreachable target or a locked vault.
- `RosterRestore` and `CliManagerTerminalRoster` gain the target-shell port.
- Web: the menu group for target sessions (from `SessionEntry.target`).

## Boundaries

- No agent tab on a target — n8 (`target-agent-tab`) owns that; `target` around an agent is refused here.
- No change to `launch_on_target` (the main) or the target main's resume.
- No sandbox tabs (n5's).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n4` resume-roster (#961) | per-kind `restore_plan`, `RosterRestore`, `restore_roster_on_resume` | adds the `target` kind and its `carry_out` arm; gives `RosterRestore` the target-shell port | change the dispatch or the outcome proto |
| `n2` tmux-roster (#959) | `tabs.yaml`, `TabRosterStore`, `CliManagerTerminalRoster` | records `[target, shell]`; adds a target branch to start | change the roster schema |
| `n5` sandbox-shell-tab (#964, contract `df710208d`) | no behaviour consumed — the line puts it below. Its delivered surface is the **pattern** this node's follows: a port in a crate both consumers already depend on (`TabJailSpawn` → `TabTargetShell` in `tddy-agent-launch`), builder injection (`DaemonSessionHost::with_tab_jail_spawn` → `with_tab_target_shell`), an `Option` field on `LaunchSessions` defaulting to the real implementation (`tab_jail_spawn()` → `tab_target_shell()`), a fake in `tests/<x>_support/`, the `TerminalKind` variant + wrapper constant, the restore kind arm beside `JailedShell` | sits beside n5's arms in `from_wrapped_request`, `restore_plan`, `carry_out`, `RosterRestore`, `CliManagerTerminalRoster::start` and `resume_roster_support` (fields added, n5's signatures unchanged); n5's own tests stay red until n5 is green, and none of this node's tests reaches an n5 `TODO` | touch `TabJails`, the sandbox kind, or any n5 symbol |
| merged `#963` (`65eeff8ef`) | the configurable start window of a target main (`RemoteLaunch::start_timeout`) | nothing — a tab has no startup window | change `launch_on_target` or its window |

## Draft PR contract

The first push publishes `launch_tab_on_target` and `TabTargetShell` with `todo!()` bodies, `TARGET_KIND`, the `TerminalKind` variant, and the failing tests below.

Published (commit 2), every body `todo!()` with `// TODO(target-shell-tab): implement`:

- **tddy-changeset** `tab_roster.rs`: `TARGET_KIND = "target"`, `LaunchNode::target()` (no properties).
- **tddy-terminal-rpc** `service/ports.rs`: `TerminalKind::TargetShell`, `TARGET_WRAPPER_KIND`, the `target` arm in `from_wrapped_request` → `TerminalKind::on_target` (todo).
- **tddy-cli-sessions** `cli_session_manager/launch.rs`: `ClientTab` and `CliSessionManager::spawn_rendered_client_tab(session_id, terminal_id, worktree, ClientTab)` — the non-main sibling of `spawn_rendered_client_main`, calling `spawn_tool_on` with its existing 13 parameters (none added).
- **tddy-agent-launch** `remote_agent_launch/target_tab.rs`: `TargetTab { shell, worktree, session_dir }`, `target_shell_tab_command(worktree)` (`cd '<wt>' && exec "${SHELL:-/bin/sh}" -l`), `launch_tab_on_target(manager, session_id, terminal_id, TargetTab)` (re-exported from `remote_agent_launch`; `launch_on_target` untouched).
- **tddy-agent-launch** `tab_target_shell.rs`: `TabTarget { shell, worktree }`, the `TabTargetShell` port (`target_of(session_dir, subject)`), `SessionTargetShells` (the daemon's, over `CoderCredentialSource`), `LaunchSessions::tab_target_shell()` and the `LaunchSessions.tab_target_shell` field.
- **tddy-agent-launch** restore: `TabRestore::TargetShell`, `target_plan` (todo), `TARGET_KIND` known, `RosterRestore.tab_target_shell`, the `carry_out` arm → `restore_target_shell` (todo).
- **tddy-session-lifecycle**: `DaemonSessionHost::with_tab_target_shell`, the roster's `target_shell` and the `TargetShell` branch of `start` → `start_target_shell_tab` (todo, `svc_terminal_ports/target_tabs.rs`); a stopped target session's host tab → `target_sessions_host_directory` (todo) behind `records_a_target`.
- **tddy-terminal-rpc** `TerminalLaunch.subject` (set in `start_terminal_session`; the three literals in `terminal_session_service_acceptance.rs` carry `GITHUB_USER`); **tddy-agent-launch** `RosterRestore.subject` and `restore_roster_on_resume(os_user, subject, …)` (both resume callers: `opencode_cli_spawn/resume.rs`, `svc_resume_sandboxed_cli_harness_session.rs`).
- **tddy-coder**: the participant's `TargetShell` refusal arm (`FAILED_PRECONDITION`).
- **tddy-session-web**: `NewTerminalMenu` `onTarget` prop, `TARGET_WRAPPER_KIND`, test ids and page-object methods for "On target" / "On this host".
- Test support: `tddy_testing_commons::remote_target` — the stand-in machine's login shell answers `-l` and records where it started (`StandInSshTarget::login_shell_started_in`); `tddy-agent-launch/tests/coder_workspace_support` — the stand-in Coder workspace **moved** out of `coder_agent_launch_acceptance.rs` (shared, not copied); `tddy-session-lifecycle/tests/target_tab_support` — `FakeTabTargetShells` and a machine on this host standing for the target.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** yes, once n4 is in — tests use the stand-in target and in-memory/recorded shells
**Concurrent with:** `sandbox-shell-tab` (n5)
**Blocks:** `target-agent-tab` (n8)

Real dependency edges: `n4 → n5, n6      n5 → n7      n6 → n8`

## Prerequisites

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md` — the tab launch is a new function sharing `client_command` (its command is a login shell, not `target_login_exec`); `launch_on_target` is not edited.
- `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` (13 params) — the non-main client spawn must not add a parameter to `spawn_tool_on`.

### ℹ ANSWERED — [2026-11-16-remote-shell-test-fixtures-are-duplicated.md](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)

New target-tab tests reuse the existing stand-in target fixture rather than adding another copy.

### ℹ ANSWERED — [2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md](../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md)

Agent tabs only; n8 claims the tab half.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [x] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

A target session's tab is a host shell in `recorded_worktree` — a path that exists only on the target. `launch_on_target` is main-only (pid file `.tddy/agents/<session>.pid`, pointer rewrite, startup window that stops the whole session). `CliManagerTerminalRoster` has no credentials or target; `RosterRestore` has no shell.

### State B (Target)

`target` → `shell` resolves the session's target shell and runs the tab on the target as a local client terminal; recorded `[target, shell]`; Resume reopens it on the target or drops it with the reason.

## Acceptance Tests

`packages/tddy-agent-launch/tests/target_shell_tab_acceptance.rs` (stand-in target):
- [x] `a_shell_tab_opened_on_target_runs_in_the_target_worktree`
- [x] `a_target_tab_writes_no_pid_file_and_leaves_the_session_pointer`
- [x] `a_target_tab_that_cannot_connect_ends_only_itself`
- [x] `a_coder_target_tab_runs_through_the_coder_client`

`packages/tddy-session-lifecycle/tests/target_shell_tab_acceptance.rs`:
- [x] `a_target_shell_tab_is_recorded_as_target_then_shell`
- [x] `an_on_this_host_tab_of_a_target_session_is_a_plain_shell` — runs in the session's directory on this host (developer ruling)
- [x] `a_target_wrapper_around_an_agent_is_refused`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [x] `a_target_shell_tab_is_reopened_on_the_target_on_resume`
- [x] `a_target_tab_whose_target_is_unreachable_is_dropped_with_its_reason`
- [x] `a_coder_target_tab_with_a_locked_vault_is_dropped_with_its_reason`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTargetAcceptance.cy.tsx`:
- [x] `offers On target (default) and On this host for a target session`
- [x] `offers no target choice for a local session` — a guard: **passes on the contract**

## Unit Tests

- The tab's remote command: `cd '<wt>' && <login exec>`, no pid-file write — `packages/tddy-agent-launch/src/remote_agent_launch/target_tab_tests.rs` (4).
- `from_wrapped_request`: `target` around `bash` accepted, around an agent refused — `packages/tddy-terminal-rpc/tests/terminal_kind_target_wrapper_unit.rs` (6).
- `restore_plan`: `[target, shell]` plans a target shell — `packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs_target_tests.rs` (4).
- `SessionTargetShells`: ssh / Coder target and worktree from `.session.yaml`, the account read from the caller's vault only, a locked vault and a missing target or worktree refused `FAILED_PRECONDITION` — `packages/tddy-agent-launch/src/tab_target_shell_tests.rs` (6).

## Decisions & Trade-offs

- **One `target` kind** (developer decision); the transport is read from `.session.yaml`, not duplicated in the node.
- **"On target" by default, "On this host" kept** (developer decision).
- **A tab failure ends only the tab** — the main and the session are untouched.

## TODO

- [x] Record initial discovery (`2026-11-21-target-shell-tab-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
