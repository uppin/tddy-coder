# A stopped session keeps its tabs — shells, harness terminals and agents - PRD

**Date**: 2026-11-02
**PRD Type**: Feature Enhancement
**Stack**: `#session-tabs 1/7` (branch `feature/session-tabs/dormant-tabs`)

## Affected Features

- **Primary**: [Session terminal tabs](../session-terminal-tabs.md) — the tab strip is shown for a stopped
  session too, and "+" opens a shell or CLI harness tab in it
- **Related**: [Inactive session activities](../inactive-session-activities.md) — the recorded
  transcript moves from covering the whole pane into the session's **Agent** tab
- **Related**: [Models and agents](../models-and-agents.md) — "Add agent" on a stopped session opens a
  conversation tab, which until now was invisible

## Summary

A session whose agent process is not running (dormant: `isActive == false`) currently loses its whole
tab strip. The web hides the runtime, and the daemon refuses `StartTerminalSession` with
`session has no running terminal`. After this change, a stopped session still has a working tab strip:

- the **Agent** tab shows the recorded transcript that the pane showed before;
- **"+"** opens bash or any CLI harness tab in the session's worktree;
- **Add agent** opens a conversation tab next to them;
- the top bar's **Resume** works as before; the tabs opened while dormant keep running and stay in the
  strip after the session resumes.

## Background

An operator who stopped a session (or whose agent exited) often still needs the worktree: to run the
tests, inspect git state, or ask another agent about it. Today the only way to get a shell there is to
resume the agent first. The daemon's refusal exists only because the roster read the worktree path off
the live main PTY (`svc_terminal_ports.rs`: "the main terminal has to exist first: it is where that
path is recorded"). The session's `.session.yaml` records the same path as `repo_path`.

## Proposed Changes

### What's Changing

- **Daemon:** `StartTerminalSession` on a session with no running main terminal resolves the worktree
  from the session's metadata (`repo_path`) in the requesting OS user's sessions directory, and starts
  the tab there.
  - A session with no worktree on this host (a split session's agent half, an unknown session id) is
    refused with an error naming why. There is no substitute directory.
- **Web:** a dormant session renders the tab strip.
  - The Agent tab holds the read-only recorded transcript (`SessionActivitiesPane`).
  - Terminal tabs stream through the owning daemon's `TerminalSessionService`, exactly like a live
    session's bash tabs.
  - Agent-conversation tabs render and work.
  - Tabs that already exist on the daemon for the session (`ListTerminalSessions`) are listed.
- After **Resume**, the strip is the live session's strip; the dormant-opened tabs are still there.

### What's Staying the Same

- Resume, its button and its RPC.
- Started tabs run as today: on the host, in the session's worktree, never inside the session's jail.
- Tabs are still held in daemon memory only; a daemon restart drops them.
- The custom workflow view still covers the pane for workflow sessions.

## Impact Analysis

### Technical Impact

- `tddy-terminal-rpc`: the roster's launch carries the requesting OS user.
- `tddy-session-lifecycle`: the daemon roster resolves the worktree from metadata when `main` is absent.
- `tddy-web`: the dormant base view becomes a tab strip whose Agent tab body is the transcript.

### User Impact

- Shells, harness terminals and agents are available on any stopped session on this host, without
  resuming it.

## Acceptance Criteria

- [ ] On a stopped (non-sandboxed) session, `StartTerminalSession` with kind bash starts a shell whose
  working directory is the session's recorded worktree.
- [ ] The same for a CLI harness kind (e.g. `warp-cli`).
- [ ] A session with no `repo_path` on this host is refused with `failed_precondition` naming the
  missing worktree.
- [ ] A live session is unaffected: its tabs still start in the live main terminal's worktree.
- [ ] Selecting a stopped session in the web shows the tab strip: Agent (with the recorded transcript),
  the "+" control, and any tabs the daemon lists for it.
- [ ] "+" → Bash on a stopped session opens a working terminal tab.
- [ ] "Add agent" on a stopped session opens a conversation tab in its strip.
- [ ] After Resume, a tab opened while the session was stopped is still in the strip.

## Out of Scope

- The greyed terminal history in the Agent tab and the resume toast (`#session-tabs 6/7`).
- Persisting tabs across daemon restarts.
- Running tabs inside a session's jail.
- Tabs for a split session's agent half, whose worktree lives on another daemon.
