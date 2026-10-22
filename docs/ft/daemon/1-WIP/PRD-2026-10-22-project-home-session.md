# Project-home session — a managed, read-only session pinned per project - PRD

**Date**: 2026-10-22
**PRD Type**: Enhancement
**Stack**: #project-home 4/7 (`feature/project-home/home-session`, base `feature/project-home/activity-sidebar`)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — a project has at most one **project-home session** per host.
- **Related Feature**: [Remote managed worktree](../remote-managed-worktree.md) / [Remote-codebase mode](../remote-codebase-mode.md) — the home session is a managed session with a **read-only** tool catalog.
- **Related Feature**: [Claude CLI session](../claude-cli-session.md) — start and resume paths it reuses.
- **Related Feature**: [Session drawer](../../web/session-drawer.md) — the home session is pinned first in its project's group.

## Summary

Every project group in the drawer starts with a pinned **main** entry. Clicking it ensures the
project's home session: the daemon starts one if none exists, resumes it if it is stopped, or returns
it if it is running — and the drawer selects it. The session runs the project's **home agent** (node 1)
in **managed-codebase mode** against a **dedicated home worktree** checked out detached at the
project's main branch ref, and its tool catalog is **read-only**: the agent can read and search the
code but cannot write, edit, delete or run a shell.

## Background

Each project needs a standing agent that can see the whole project — its main branch and, through
later nodes, its other sessions — without being able to damage either. Pinning it in the drawer
makes it the default place to start; read-only makes it safe to leave around.

## Proposed Changes

### What's Changing
- New RPC `SessionService.EnsureProjectHomeSession(project_id, daemon_instance_id)` →
  `{ session: SessionEntry, outcome: "started" | "resumed" | "running" }`.
  - **No home agent set** → `failed_precondition` ("set a home agent for this project").
  - **Home agent whose harness cannot enforce managed-codebase mode** (cursor-cli today) →
    `failed_precondition` naming the harness. Nothing is started.
  - **None exists** → create the home worktree (detached at `main_branch_ref`, after fetching it) and
    start a managed session on it with the home agent, `session_role = "project-home"`.
  - **Stopped** → fast-forward the home worktree to the fetched `main_branch_ref`, then `ResumeSession`
    (same agent conversation).
  - **Running** → return it unchanged.
- `SessionEntry.session_role` (`"project-home"`, or empty for every other session), persisted in `.session.yaml`.
- Read-only catalog: for a `project-home` session the exec tools that modify the worktree or run
  processes are neither advertised nor executable — `ExecuteTool` refuses them by role, so the guarantee
  does not rest on the agent's allowlist alone.
- Drawer: each **project** group's first row is the pinned entry (label `main`, the main branch ref,
  the home agent's label, the session's status dot once it exists). The home session is not repeated
  in the group's rows. Clicking it calls `EnsureProjectHomeSession` and selects the result; a refusal is
  shown on the entry. A project with no home agent shows a hint pointing to the Projects screen. The
  **Other** group has no pinned entry.
- Because the home session is managed, it opens with the activity sidebar (node 3).

### What's Staying the Same
- Every other session's start, resume, tools and placement.
- Deleting the home session is allowed; the next click creates a new one.
- Auto-stop is node 6; tools to manage other sessions are nodes 5 and 7.

## Impact Analysis

### Technical Impact
- Proto: field + RPC. Daemon: an ensure service in agent-launch, a home-worktree helper in
  worktree-service, a role filter in the tool engine, metadata/reader/enrichment fields. Web: pinned entry.
- One extra worktree per project that uses the feature.

### User Impact
- A one-click, always-there agent per project. No migration.

## Implementation Plan
1. Role field (metadata → reader → proto).
2. Home worktree create / fast-forward.
3. Ensure service: decide started/resumed/running; refusals.
4. Read-only role filter in advertisement and `ExecuteTool`.
5. Pinned drawer entry.

## Acceptance Criteria
- [ ] Ensuring a project's home starts a managed session on a home worktree at the main ref, listed with `session_role = "project-home"`.
- [ ] Ensuring again while it runs returns the same session (`running`).
- [ ] Ensuring a stopped home session fast-forwards its worktree and resumes the same conversation (`resumed`).
- [ ] Ensuring with no home agent, or with one that cannot enforce managed mode, is refused and starts nothing.
- [ ] A project-home session's write/shell exec tools are refused by the daemon.
- [ ] Each project group shows the pinned `main` entry first; clicking it ensures and selects the home session; the session is not repeated below.
- [ ] The Other group has no pinned entry; a project with no home agent shows the hint.

## References

### Affected Features (Complete List)
- [project-concept.md](../project-concept.md), [remote-managed-worktree.md](../remote-managed-worktree.md), [claude-cli-session.md](../claude-cli-session.md), [session-drawer.md](../../web/session-drawer.md)

### Related Documentation
- Predecessors: `feature/project-home/home-agent-setting` (1/7), `feature/project-home/project-groups` (2/7), `feature/project-home/activity-sidebar` (3/7).
- Successors: `feature/project-home/home-session-tools` (5/7), `feature/project-home/home-auto-stop` (6/7).
