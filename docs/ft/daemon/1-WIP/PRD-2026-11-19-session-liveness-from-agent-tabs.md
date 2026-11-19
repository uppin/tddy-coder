# Session liveness from live agent tabs - PRD

**Date**: 2026-11-19
**Status**: Draft
**Changeset**: [2026-11-19-session-liveness-from-agent-tabs.md](../../../dev/1-WIP/2026-11-19-session-liveness-from-agent-tabs.md)

## Affected Features

- [Session terminal tabs](../../web/session-terminal-tabs.md) — § Stopped sessions
- [Inactive session activities](../../web/inactive-session-activities.md) — what "inactive" means
- [Terminal sessions](../terminal-sessions.md)
- [Project-home auto-stop](../project-home-auto-stop.md)
- [Session branch conflict](../session-branch-conflict.md) — an *active* owner refuses a start

## Summary

A session is **active if and only if the daemon holds at least one live agent tab for it**: its
main agent, or any extra agent terminal tab. The pid recorded in `.session.yaml` no longer decides
it. Delete, SignalSession and the project-home TerminateSession act only on processes the daemon
holds.

## Background

Liveness was `kill(pid, 0)` on the single pid in `.session.yaml`. Nothing clears that pid when the
agent exits, so when the OS recycles it for an unrelated process a closed session reappears in the
drawer as **active** — briefly for a short-lived new owner (rustc, git), for good for a long-lived
one. Reported for `01a1e86c-28ed-7e72-8427-08b7665de3a4`: Status `active`, PID 22059, Activity
`Ended`. The same pid was also *signalled* by delete and SignalSession, so a delete could kill an
unrelated process.

## Proposed Changes

### What's Changing

- **The rule.** Active ⇔ ≥ 1 live **agent tab** held by this daemon. An agent tab is:
  - the session's `main` terminal (local PTY agent, tmux client of a tmux-run agent, or the
    transport client of a target session);
  - an agent terminal tab (`StartTerminalSession` with a harness `kind`);
  - a sandboxed session's runner, while it runs;
  - a tmux-run agent whose client ended while this daemon watches its tmux session (detached).
  Bash and tmux-attach tabs are not agents and never make a session active.
- **A daemon restart** starts with nothing held: every session lists inactive, whatever its
  `.session.yaml` says, until it is started, resumed or re-attached. A tmux-run agent that outlived
  the daemon lists inactive until its main is re-attached (opening its terminal, or Resume).
- **Main exited, an agent tab still running** → the session is active.
- **Delete** stops every terminal the session has (main and tabs) and its sandbox, and never
  signals the recorded pid.
- **SignalSession / TerminateSession** signal the session's held main agent; with none held they are
  refused (`process is not alive`), and the recorded pid is never signalled.
- **Every consumer** of session liveness follows the rule: the web drawer (`ListSessions`),
  `ListProjectSessions` and the project soft-delete refusal, branch-ownership conflicts, project-home
  ensure (a main whose recorded pid is alive but whose terminal nobody serves is now **resumed**) and
  the idle reaper, project-home session tools, session git ops, the PR-status tools and Telegram.

### What's Staying the Same

- `.session.yaml` keeps `pid` as a record of what was started. Nothing reads it to decide liveness.
- `tool` (tddy-coder) sessions: see the interim rule below.

### Interim (until stack node 3)

`tool` sessions have no daemon-held process yet — the child lives in the supervisor, the spawn
worker or a detached `wait()` thread. Until node 3 makes a tool session a tab in the same registry,
a running tool session **lists inactive**, and delete / SignalSession still reach a tool session
through its recorded pid (the only handle there is). Recorded as a TODO naming node 3.

## Stack

| Node | Branch | Scope |
|---|---|---|
| **1 (this)** | `feature/session-liveness/agent-tab-liveness` | Liveness from live agent tabs; delete / signal / reaper through held processes |
| 2 | `feature/session-liveness/tmux-roster` | A per-session tab roster in `.session.yaml`; agent tabs wrapped in tmux (a wrapper chain: tmux wraps agent, later ssh wraps tmux wraps agent); eager re-attach of every live tmux-wrapped tab at daemon startup |
| 3 | `feature/session-liveness/tool-tab` | `tool` sessions as a process tab, with an exit signal from each spawn backend |
| 4 | `feature/session-liveness/resume-roster` | Resume restores the whole roster under the recorded terminal ids, resuming each agent tab's own conversation |

## Acceptance Criteria

1. A session whose `.session.yaml` records a pid that belongs to a live unrelated process, with no
   live agent tab, lists **inactive**.
2. A session whose main agent runs lists **active**, whatever (or whether) a pid is recorded.
3. A session whose main agent exited while a second agent tab runs lists **active**.
4. A session whose agent tabs have all closed lists **inactive**.
5. A session with only a shell tab lists **inactive**.
6. After a daemon restart a session lists **inactive**.
7. A tmux-run agent that outlived the daemon lists inactive until its main is re-attached, then active.
8. Deleting a session leaves the unrelated process its recorded pid names running, and stops the
   session's agent tabs.
9. SignalSession reaches the session's running main agent, never the recorded pid's process; with no
   held agent it is refused.
10. Ensuring a project-home main whose recorded pid is alive but whose terminal nobody serves
    **resumes** it.
11. A sandboxed session whose runner exited lists inactive.
