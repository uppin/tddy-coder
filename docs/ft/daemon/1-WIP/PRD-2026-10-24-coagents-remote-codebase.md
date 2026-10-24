# Coding subagents on the codebase host, in remote-codebase mode - PRD

**Date**: 2026-10-24
**PRD Type**: Enhancement
**Stack**: `#coagents` 6/8, branch `feature/coagents/remote-codebase`, base `feature/coagents/clone-placement` (#724)

## Affected Features

- **Primary Feature**: [Remote-codebase mode](../remote-codebase-mode.md) — a workspace session can carry coding subagents that run on the codebase host
- **Related Feature**: [Session agent roster](../session-agent-roster.md) — rosters on `workspace` sessions (previously a non-goal) for coding agents
- **Related Feature**: [Managed-codebase subagents](../../coder/managed-codebase-subagents.md) — `subagent_*` tools over the relay transport

## Summary

In remote-codebase mode the main agent runs on one machine and the codebase lives in a `workspace`
session on a remote daemon; every file and shell operation goes through `tddy-tools`, the local relay
daemon and `ExecuteTool` on the codebase host. This PRD lets that main agent open **coding subagents
that run on the codebase host**, against the workspace session's own worktree — where the files, the
toolchain and the build cache are — instead of on the main agent's machine where there is no
checkout at all. Opting in is per session: a remote-codebase session started with coding agents
named on it.

## Background

Today a `workspace` session has no roster ("A session with no agent has no roster"), and the relay
transport `tddy-tools` uses in this mode refuses the roster and conversation RPCs
(`DaemonHttp` is `ExecuteTool`-only). So `subagent_*` tools are never advertised in remote-codebase
mode. The codebase host is exactly where a coding agent can do real work.

## Proposed Changes

### What's Changing

- **Opt in at start.** `tddy-tools remote start-session` (and `StartSession` with
  `session_type: "workspace"`) accepts coding agents — qualified ids of agents offered by the codebase
  daemon (`name@codebase-daemon`) — and seeds the workspace session's roster with them. A workspace
  session started without any keeps having no roster.
- **Only on the codebase host.** A workspace session's roster holds `coding_agent` entries owned by
  that same daemon; an entry naming another daemon or a `model` agent is refused naming it.
- **The relay carries conversations.** `tddy-tools --mcp` in remote-codebase mode streams the
  workspace session's roster and opens, prompts, resumes, awaits and cancels conversations through the
  relay to the codebase daemon; the `subagent_*` tools are advertised as soon as the roster is not
  empty.
- **The agent runs where the code is.** The codebase daemon launches the coding agent as the
  workspace session's OS user, on the workspace worktree (or its own clone with `clone` placement),
  under managed confinement — exactly as for a local session.
- **Permissions and the activity bubble work unchanged** — they ride the same conversation RPCs.

### What's Staying the Same

- Remote-codebase sessions without coding agents.
- Split sessions (remote-managed worktree) — their roster already lives on the agent daemon.
- The roster's "no write-back from a remote clone" rule — this agent works on the authoritative
  worktree, not on a clone of someone else's.

## Impact Analysis

### Technical Impact

- `tddy-session-tool-client`: the relay transport allows the `session_agents` conversation RPCs.
- `tddy-daemon` relay: forwards `session_agents.SessionAgentService` to the remote peer.
- `tddy-session-agents`: rosters on `workspace` sessions, coding agents only, local owner only.
- `tddy-tools`: `remote start-session --agent`.

### User Impact

- An operator driving a remote codebase from a laptop can hand a build-and-test task to a coding agent
  on the workstation that holds the code.

## Acceptance Criteria

- [ ] `remote start-session --agent codex@ws` starts a workspace session whose roster holds that coding agent.
- [ ] A workspace session refuses a `model` agent or an agent owned by another daemon, naming it.
- [ ] `tddy-tools --mcp` over the relay advertises the `subagent_*` tools for that session.
- [ ] A conversation opened through the relay runs on the codebase host against the workspace worktree, as the session user.
- [ ] Prompt, await, permission resume and cancel work through the relay.
- [ ] A workspace session without agents still has no roster and no `subagent_*` tools.

## References

- [Remote-codebase mode](../remote-codebase-mode.md) (note its criteria 23–28: the `tddy-coder --remote` bootstrap is still unimplemented; this PRD targets the relay + `tddy-tools remote` path that is)
- Stack parents: #720–#724
