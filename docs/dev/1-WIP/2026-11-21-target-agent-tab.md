# Changeset: A target session's agent tab runs on the target

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, terminal-rpc, session-lifecycle, session-web, plus the participant arm in tddy-coder), agent binary and conversation resolution moved off the daemon host for tabs (unchanged at the contract: the launch reuses n6's non-main client spawn and port; the new work is the bare-command tab and its carried environment)
**Stack**: `#session-liveness` 8/8 — branch `feature/session-liveness/target-agent-tab`, base `feature/session-liveness/sandbox-agent-tab`
**PR**: #967 https://github.com/uppin/tddy-coder/pull/967

## Initial Discovery

[2026-11-21-target-agent-tab-initial-discovery.md](./2026-11-21-target-agent-tab-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: a target agent tab (bare command, resolved by the target's login `PATH`; chat minted on the target; rendered env carried in the command); `[target, agent]` restore
- **tddy-terminal-rpc**: `target` around an agent accepted
- **tddy-session-lifecycle**: roster start of a target agent tab
- **tddy-session-web**: agent kinds listed under "On target"
- **tddy-coder**: the participant roster refuses `target` around an agent (an arm forced by the new `TerminalKind` variant)

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-agent-tab.md](../../ft/daemon/1-WIP/PRD-2026-11-21-target-agent-tab.md)

## Summary

An agent tab opened "On target" runs the agent on the target in the session's worktree, starts even when the daemon host has no agent, mints its chat on the target, and is reopened there by Resume.

## Responsibility

- A target variant of `new_agent_tab`: the argv names the spec's bare command (no host `coding_agents.resolve`), the conversation is minted with `ConversationStart { target: Some(..) }`, the rendered env goes into the remote command as the main's does (`target_login_exec`).
- Launched through n6's `launch_tab_on_target`.
- Recording `[target, agent]`; restore resumes the conversation on the target where the harness can, otherwise reopens fresh.
- `target` around an agent accepted; agents listed under "On target".

## Boundaries

- Does not change n6's launch, port or shell tabs.
- Does not change the target **main**'s binary resolution (the start half of the todo below stays open).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n6` target-shell-tab | `launch_tab_on_target`, `TabTargetShell`, `TARGET_KIND`, "On target" group | launches agent tabs through them | change the launch, the port or the kind |
| `n4` resume-roster (#961) | `new_agent_tab`, `agent_launch_node`, `restore_plan` | adds the target variant of the agent-tab rule | change the host rule |
| `n6` target-shell-tab (#965, contract `3845d5f75`) | `spawn_rendered_client_tab` + `ClientTab` (the non-main client spawn), the private `client_command` of `remote_agent_launch` (the transport client of a target command), `TabTarget` / `TabTargetShell` and its lifecycle wiring (`with_tab_target_shell`, `RosterRestore.tab_target_shell` / `.subject`, `TerminalLaunch.subject`), `TARGET_WRAPPER_KIND`, `LaunchNode::target()`, the "On target" / "On this host" menu groups, the test fixtures (`remote_target::StandInSshTarget`, `target_tab_support::fake_target_shells`) | `launch_agent_tab_on_target` launches through the same non-main client spawn as `launch_tab_on_target`; the roster and the restore resolve the target through the same port; the agent kinds join n6's menu groups; the fake target's login shell is extended to answer `-lc` (additive) | change any n6 signature; edit an n6 `todo!()` body |
| `n7` sandbox-agent-tab | nothing consumed — the line puts it below | — | — |

**Sequencing of this node's tests on n6's green** (pass 2, 2026-11-21). Each n8 path sits *ahead* of
n6's `todo!()` instead of inside it (`from_wrapped_request` → `on_target_agent` before `on_target`;
`restore_plan` → `TARGET_KIND if wraps_an_agent` → `target_agent_plan` before `target_plan`;
`TerminalRoster::start` → `start_target_agent_tab`), so every n8 test fails on n8's own todo today.
The unit suites (`restore_tabs_target_agent_tests`, `terminal_kind_target_agent_unit`) pass on n8's
green alone. These pass only once n6 is green too, because the integration *is* the point:

- `target_agent_tab_acceptance` (tddy-agent-launch, all four) — the tab's terminal is n6's
  `CliSessionManager::spawn_rendered_client_tab`.
- `target_agent_tab_acceptance::a_target_agent_tab_is_recorded_as_target_then_agent` and
  `resume_tab_roster_acceptance::a_target_claude_tab_is_reopened_on_the_target_resuming_its_conversation`
  (tddy-session-lifecycle) — the same spawn, behind the roster start and the restore.
- Cypress `lists agent kinds under On target for a target session` — n6's "On target" / "On this
  host" groups.

n8 inverts three n6 tests and deletes them:
`terminal_kind_target_wrapper_unit::an_agent_wrapped_in_target_is_refused_naming_the_agent`,
`restore_tabs_target_tests::an_agent_recorded_on_a_target_is_dropped_naming_the_agent`, and
`target_shell_tab_acceptance::a_target_wrapper_around_an_agent_is_refused` (tddy-session-lifecycle;
replaced by `target_agent_tab_acceptance::a_target_agent_tab_is_recorded_as_target_then_agent`).

## Draft PR contract

The first push publishes the target variant of the agent-tab rule as `todo!()` and the failing tests below.

Published surface (bodies `todo!()`, `TODO(target-agent-tab)`, unless marked):

- tddy-terminal-rpc `service/ports.rs`: `TerminalKind::TargetAgent { agent }`; `on_target_agent`, reached from `from_wrapped_request` ahead of n6's `on_target`.
- tddy-agent-launch `remote_agent_launch/target_agent_tab.rs`: `TargetAgentTab { kind, argv, env }`, `launch_agent_tab_on_target(manager, session_id, terminal_id, TargetTab, &TargetAgentTab)` (re-exported from `remote_agent_launch`; n6's `TargetTab` reused unchanged; `launch_on_target` untouched).
- tddy-agent-launch `restore_tabs/new_target_tab.rs`: `target_agent_tab(coding_agents, harness, conversation_id, resume)` (the bare-command argv and the carried env; no `resolve`), `new_target_agent_tab(coding_agents, harness, &TabTarget)` (the conversation settled with `ConversationStart { target: Some(..) }`), `NewTargetAgentTab { agent, conversation_id, resumable }` (re-exported from `restore_tabs`).
- tddy-agent-launch restore: `TabRestore::TargetAgent { harness, resume_conversation }`, `target_agent_plan` (dispatched ahead of n6's `target_plan`), the `carry_out` arm → `restore_target_agent`.
- tddy-session-lifecycle `svc_terminal_ports/target_agent_tabs.rs`: `start_target_agent_tab`, routed from `TerminalRoster::start`; `start_tab`'s `TargetAgent` arm is an internal error (implemented, like n6's).
- tddy-coder participant: the `TargetAgent` refusal arm (`FAILED_PRECONDITION`, implemented).
- tddy-session-web: `TODO(target-agent-tab)` on `NewTerminalMenu`'s `onTarget` prop.
- Test support: `fake_target_shells::AStandInTarget` — its login shell also answers `-lc` with the machine's `opt/agents` first on `PATH`; `installs_the_agent` / `agent_run`; `target_tab_support::where_the_agent_ran`.
- Left to green: sharing `write_rendered_session_files` (today `pub(super)` over the main's `RemoteLaunch`) with the tab, and the `wrappers` proto comment (regenerates TS).

## Green wave

**Wave:** 5 of 5
**Greenable independently:** no — it launches through n6
**Concurrent with:** `sandbox-agent-tab` (n7)
**Blocks:** none

Real dependency edges: `n4 → n5, n6      n5 → n7      n6 → n8`

## Prerequisites

### ✅ RESOLVED HERE (the new-tab half) — [2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md](../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md)

A target agent tab no longer resolves the binary on the daemon host. The session-**start** half stays open — narrow the entry at wrap.

### ⚠ DURING — [2026-11-19-coder-remote-agent-local-paths.md](../todo/2026-11-19-coder-remote-agent-local-paths.md)

A target agent tab must not hand the target a host path (rendered session files go to the target worktree's `.tddy/`, as `write_rendered_session_files` does for the main).

## Scope

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests / run them (verify they fail) / USER REVIEW
- [x] TDD Red — failing unit/integration tests
- [x] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

After n6, `target` around an agent is refused. `new_agent_tab` resolves the binary on the daemon host and mints the chat with `target: None`.

### State B (Target)

`target` → `agent` launches the agent on the target with a bare command and a chat minted there; Resume reopens it on the target.

## Acceptance Tests

`packages/tddy-agent-launch/tests/target_agent_tab_acceptance.rs` (stand-in target):
- [x] `an_agent_tab_opened_on_target_runs_the_agent_in_the_target_worktree`
- [x] `a_target_agent_tab_starts_when_the_daemon_host_has_no_agent`
- [x] `a_target_agent_tabs_chat_is_minted_on_the_target`
- [x] `a_target_agent_tab_hands_the_target_no_host_path`

`packages/tddy-session-lifecycle/tests/target_agent_tab_acceptance.rs` (added at the contract — the roster start and its record, replacing n6's refusal test):
- [x] `a_target_agent_tab_is_recorded_as_target_then_agent`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [x] `a_target_claude_tab_is_reopened_on_the_target_resuming_its_conversation`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTargetAcceptance.cy.tsx` (extended):
- [x] `lists agent kinds under On target for a target session`

## Unit Tests

- The target agent tab's argv: the bare command, `resume` from the recorded conversation; its kind and carried env; an unknown harness refused — `packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs_target_agent_tests.rs` (6).
- `restore_plan`: `[target, agent]` plans a target resume or fresh agent; a missing harness or conversation, or a node inside the agent, dropped — same file (5).
- `from_wrapped_request`: `target` around an agent kind is `TargetAgent`; a wrapper inside the target refused naming it — `packages/tddy-terminal-rpc/tests/terminal_kind_target_agent_unit.rs` (4).

## Decisions & Trade-offs

- **The target resolves the agent** (its login `PATH`), as the target main does after #955.

## TODO

- [x] Record initial discovery (`2026-11-21-target-agent-tab-initial-discovery.md`)
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
