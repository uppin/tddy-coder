# Changeset: A target session's shell tab runs on the target

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (terminal-rpc, changeset, agent-launch, session-lifecycle, session-web), a non-main target client launch beside a main-only one, vault/credential wiring into the roster
**Stack**: `#session-liveness` 6/8 — branch `feature/session-liveness/target-shell-tab`, base `feature/session-liveness/sandbox-shell-tab`

## Initial Discovery

[2026-11-21-target-shell-tab-initial-discovery.md](./2026-11-21-target-shell-tab-initial-discovery.md)

## Affected Packages

- **tddy-terminal-rpc**: `target` accepted around a shell
- **tddy-changeset**: `TARGET_KIND` and `LaunchNode::target()`
- **tddy-agent-launch**: `launch_tab_on_target` (non-main target client), `TabTargetShell` port (resolve the session's target shell with the owner's credentials), `target` restore kind
- **tddy-session-lifecycle**: roster start with the target shell; the target worktree (`repo_path`) as the tab's directory
- **tddy-session-web**: "On target" / "On this host" menu group
- **tddy-coder**: the participant roster refuses `target`

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-shell-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-shell-tab.md)

## Summary

A shell tab opened "On target" on an ssh or Coder target session runs `cd <worktree> && <login shell>` on the target through the session's remote shell, as a local tab terminal. Recorded `[target, shell]`, reopened on the target by Resume, dropped with its reason when the target cannot be reached.

## Responsibility

- `launch_tab_on_target(shell, worktree, terminal_id, …)` in `remote_agent_launch`: builds the tab's remote command from `target_login_exec` (no pid file, no pointer, no startup window, never `stop_session`), runs it through `RemoteShell::interactive` in a non-main local PTY (a non-main sibling of `spawn_rendered_client_main`).
- `TabTargetShell` port: resolve a session's target shell from `.session.yaml` `target` with `shell_for_session_target`, the owner's subject and `CoderCredentialSource`; a locked vault is `failed_precondition`.
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
| `n5` sandbox-shell-tab | nothing consumed — the line puts it below | — | touch `TabJails` or the sandbox kind |

## Draft PR contract

The first push publishes `launch_tab_on_target` and `TabTargetShell` with `todo!()` bodies, `TARGET_KIND`, the `TerminalKind` variant, and the failing tests below.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** yes, once n4 is in — tests use the stand-in target and in-memory/recorded shells
**Concurrent with:** `sandbox-shell-tab` (n5)
**Blocks:** `target-agent-tab` (n8)

Real dependency edges: `n4 → n5, n6      n5 → n7      n6 → n8`

## Prerequisites

### Code issues (⚠ DURING — must not grow)

- `packages/tddy-agent-launch/docs/code-issues/complexity-remote-agent-launch-launch-on-target.md` — the tab launch is a new function sharing `target_login_exec` / `client_command`; `launch_on_target` is not edited.
- `packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md` (13 params) — the non-main client spawn must not add a parameter to `spawn_tool_on`.

### ℹ ANSWERED — [2026-11-16-remote-shell-test-fixtures-are-duplicated.md](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)

New target-tab tests reuse the existing stand-in target fixture rather than adding another copy.

### ℹ ANSWERED — [2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md](../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md)

Agent tabs only; n8 claims the tab half.

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

A target session's tab is a host shell in `recorded_worktree` — a path that exists only on the target. `launch_on_target` is main-only (pid file `.tddy/agents/<session>.pid`, pointer rewrite, startup window that stops the whole session). `CliManagerTerminalRoster` has no credentials or target; `RosterRestore` has no shell.

### State B (Target)

`target` → `shell` resolves the session's target shell and runs the tab on the target as a local client terminal; recorded `[target, shell]`; Resume reopens it on the target or drops it with the reason.

## Acceptance Tests

`packages/tddy-agent-launch/tests/target_shell_tab_acceptance.rs` (stand-in target):
- [ ] `a_shell_tab_opened_on_target_runs_in_the_target_worktree`
- [ ] `a_target_tab_writes_no_pid_file_and_leaves_the_session_pointer`
- [ ] `a_target_tab_that_cannot_connect_ends_only_itself`
- [ ] `a_coder_target_tab_runs_through_the_coder_client`

`packages/tddy-session-lifecycle/tests/target_shell_tab_acceptance.rs`:
- [ ] `a_target_shell_tab_is_recorded_as_target_then_shell`
- [ ] `an_on_this_host_tab_of_a_target_session_is_a_plain_shell`
- [ ] `a_target_wrapper_around_an_agent_is_refused`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [ ] `a_target_shell_tab_is_reopened_on_the_target_on_resume`
- [ ] `a_target_tab_whose_target_is_unreachable_is_dropped_with_its_reason`
- [ ] `a_coder_target_tab_with_a_locked_vault_is_dropped_with_its_reason`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTargetAcceptance.cy.tsx`:
- [ ] `offers On target (default) and On this host for a target session`
- [ ] `offers no target choice for a local session`

## Unit Tests

- The tab's remote command: `cd '<wt>' && <login exec>`, no pid-file write.
- `from_wrapped_request`: `target` around `bash` accepted, around an agent refused.
- `restore_plan`: `[target, shell]` plans a target shell.

## Decisions & Trade-offs

- **One `target` kind** (developer decision); the transport is read from `.session.yaml`, not duplicated in the node.
- **"On target" by default, "On this host" kept** (developer decision).
- **A tab failure ends only the tab** — the main and the session are untouched.

## TODO

- [x] Record initial discovery (`2026-11-21-target-shell-tab-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
