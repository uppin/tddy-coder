# Orchestrator sessions: any number of read-only sessions with main's reach — PRD

**Date**: 2026-11-10
**PRD Type**: Enhancement
**Stack**: `#orchestrators` 1/11 (`feature/orchestrators/orchestrator-role`)

## Affected Features

- **Primary**: [Project concept § Project-home session](../project-concept.md#project-home-session) and
  [§ Project-home tools](../project-concept.md#project-home-tools) — a second role, `orchestrator`, that
  carries main's placement and tool set
- [Project-home auto-stop](../project-home-auto-stop.md) — unchanged rule, stated: main only
- [Session drawer](../../web/session-drawer.md) — a row marker for orchestrator sessions; the
  New-session form's **Session orchestrator** toggle

## Summary

A project can run **any number of orchestrator sessions** beside its main. An orchestrator is an
ordinary session the operator (or main, or another orchestrator) starts with the **Session
orchestrator** option. The daemon places it exactly where main lives — the project's home worktree,
managed codebase, read-only, `claude-cli` (any agent whose spec declares `project_home_main`) — and
gives it main's tools: `ListProjectSessions`, `StartSession`, the follow, terminal, answer, notification
and git tools. Main stays what it is: pinned, recorded as `home_session_id`, reached by
`EnsureProjectHomeSession`, replaced from the setup pane, and the only session auto-stopped when idle.

## Background

Today a project has exactly one project-home session per host. An operator who wants two conversations
steering the project — one shepherding a stack, one triaging — has to give one of them a writable
worktree and lose the read-only guarantee, or replace main and lose its conversation.

## Proposed Changes

### What's Changing

- **Role `orchestrator`** in `.session.yaml` `session_role`, beside `project-home`. Both are
  *orchestrator roles*; every place that today grants main's reach by matching `project-home` grants it
  to either: the `ExecuteTool`/`StreamExecuteTool` read-only refusal, `ListExecTools`'s offered set, the
  `--allowedTools`/`--disallowedTools`/`tddy-tools --mcp`-only launch, the agent credential, the role
  prompt, and the cross-host owner check.
- **Requested on start.** `StartSessionRequest.session_role = "orchestrator"` is accepted from a person
  (a web or CLI caller) and from an agent whose own session holds an orchestrator role. Any other agent
  caller is refused with `PERMISSION_DENIED`. `project-home` stays daemon-only: a request naming it is
  refused as today.
- **The daemon places it.** As for main: `project_id` from the request, `repo_path` = the home worktree
  (created at the main branch ref when absent, fast-forwarded on every start and resume),
  `managed_codebase = true`, branch and stack fields cleared, a type that cannot be held read-only
  refused with `FAILED_PRECONDITION`, any other placement (`sandboxed_codebase`,
  `codebase_daemon_instance_id`, `ssh_config_host`, `agent_clone`, `split_agent`) refused with
  `INVALID_ARGUMENT` naming the field.
- **Shared worktree, separate reach.** Several orchestrators and main share one home worktree. A
  worktree-writing tool (the git tools) whose target is another orchestrator-role session is refused —
  its worktree is the home worktree. Read tools may target it.
- **Main and orchestrators start orchestrators.** The exec `StartSession` tool offers `session_role`
  with the single value `orchestrator`.
- **The orchestrator link is written at start.** A session started by an orchestrator carries
  `orchestrator_session_id` in its initial record, before its agent runs.
- **Web**: the New-session form gains a **Session orchestrator** switch (claude-cli only); on, it hides
  the placement, branch and stack fields as main's setup form does. A drawer row whose session has the
  orchestrator role shows an *orchestrator* marker.

### What's Staying the Same

- Main: `home_session_id`, `EnsureProjectHomeSession`, Replace main, its pinned entry.
- Auto-stop matches `project-home` only, so orchestrators are never idle-stopped.
- The project-home tool set itself, its targets, paths and one-hop forwarding.

## Impact Analysis

### Technical Impact
- `tddy-changeset` (role constant + predicate), `tddy-agent-launch` (role refusal, placement, launch
  wiring), `tddy-tool-engine` (catalogue role, `StartSession` arg), `tddy-daemon-rpc` (guard, caller
  gate, owner check, write-target refusal), `tddy-session-create-web`, `tddy-session-web`.
- No proto field: `session_role` (43) and `SessionEntry.session_role` (35) carry the new value.

### User Impact
- New: as many read-only steering sessions per project as wanted. No migration.

## Implementation Plan
1. One predicate `is_orchestrator_role` replacing the six exact-string gates.
2. Requested-role admission in `refuse_a_requested_role`, by caller kind.
3. Placement shared with main (`on_home_worktree`).
4. Write-target refusal; `StartSession` argument; start-time orchestrator link.
5. Web toggle and marker.

## Acceptance Criteria
- [ ] A person starts a session with `session_role: orchestrator`; it runs on the home worktree, read-only, with main's tools
- [ ] An ordinary agent asking for the role is refused; an orchestrator or main agent is not
- [ ] `project-home` requested on `StartSession` is still refused
- [ ] Two orchestrators and main run at once on one home worktree
- [ ] A git tool targeting another orchestrator is refused naming why
- [ ] A forwarded call from an orchestrator is served by the other host
- [ ] Idle auto-stop never stops an orchestrator
- [ ] A session an orchestrator starts carries `orchestrator_session_id` before its agent runs
- [ ] The New-session form starts an orchestrator; the drawer marks it

## References
- [Project concept](../project-concept.md), [Project-home auto-stop](../project-home-auto-stop.md),
  [Session drawer](../../web/session-drawer.md)
