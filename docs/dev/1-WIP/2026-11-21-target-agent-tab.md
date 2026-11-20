# Changeset: A target session's agent tab runs on the target

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — several crates (agent-launch, terminal-rpc, session-lifecycle, session-web), agent binary and conversation resolution moved off the daemon host for tabs
**Stack**: `#session-liveness` 8/8 — branch `feature/session-liveness/target-agent-tab`, base `feature/session-liveness/sandbox-agent-tab`

## Initial Discovery

[2026-11-21-target-agent-tab-initial-discovery.md](./2026-11-21-target-agent-tab-initial-discovery.md)

## Affected Packages

- **tddy-agent-launch**: a target agent tab (bare command, resolved by the target's login `PATH`; chat minted on the target; rendered env carried in the command); `[target, agent]` restore
- **tddy-terminal-rpc**: `target` around an agent accepted
- **tddy-session-lifecycle**: roster start of a target agent tab
- **tddy-session-web**: agent kinds listed under "On target"

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
| `n7` sandbox-agent-tab | nothing consumed — the line puts it below | — | — |

## Draft PR contract

The first push publishes the target variant of the agent-tab rule as `todo!()` and the failing tests below.

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
- [ ] TDD Red — failing unit/integration tests
- [ ] Implementation
- [ ] Package documentation, scoped clippy, single-spec Cypress

## Technical Changes

### State A (Current)

After n6, `target` around an agent is refused. `new_agent_tab` resolves the binary on the daemon host and mints the chat with `target: None`.

### State B (Target)

`target` → `agent` launches the agent on the target with a bare command and a chat minted there; Resume reopens it on the target.

## Acceptance Tests

`packages/tddy-agent-launch/tests/target_agent_tab_acceptance.rs` (stand-in target):
- [ ] `an_agent_tab_opened_on_target_runs_the_agent_in_the_target_worktree`
- [ ] `a_target_agent_tab_starts_when_the_daemon_host_has_no_agent`
- [ ] `a_target_agent_tabs_chat_is_minted_on_the_target`
- [ ] `a_target_agent_tab_hands_the_target_no_host_path`

`packages/tddy-session-lifecycle/tests/resume_tab_roster_acceptance.rs` (extended):
- [ ] `a_target_claude_tab_is_reopened_on_the_target_resuming_its_conversation`

`packages/tddy-session-web/cypress/component/NewTerminalMenuTargetAcceptance.cy.tsx` (extended):
- [ ] `lists agent kinds under On target for a target session`

## Unit Tests

- The target agent tab's argv: the bare command, `resume` from the recorded conversation.
- `restore_plan`: `[target, agent]` plans a target resume or fresh agent.

## Decisions & Trade-offs

- **The target resolves the agent** (its login `PATH`), as the target main does after #955.

## TODO

- [x] Record initial discovery (`2026-11-21-target-agent-tab-initial-discovery.md`)
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
