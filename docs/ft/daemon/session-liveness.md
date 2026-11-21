# Session liveness

**Status:** ✅ Shipped (`#session-liveness` 1/4, PR #958)

## Summary

A session is **active if and only if the daemon holds at least one live agent tab for it**: its main
agent, or any extra agent terminal tab. The pid a session's `.session.yaml` records is a record of
what was started and never decides liveness, because nothing clears it when the agent exits and the
OS reissues it to an unrelated process. Delete, `SignalSession` and the project-home
`TerminateSession` act only on processes the daemon holds.

## The rule

The daemon asks its own holders which sessions are live — the terminal manager and the sandbox
manager — and a session listing marks a session `is_active` when its id is in that set. Liveness is
per daemon: a session is active on the daemon that holds its agent, and nothing reads another host's
processes.

| Kind of session | Live while |
|---|---|
| Local PTY main (project-home, orchestrator, split agent half) | its `main` terminal resolves |
| tmux-run main | its tmux client `main` resolves, or a detach watcher holds it; after a restart, once re-attached |
| Target (ssh / Coder) | its transport-client `main` resolves — it exits with the remote agent |
| Sandboxed | its runner process has not exited |
| Agent terminal tab (a CLI harness `kind`) | its terminal resolves |
| Bash or tmux-attach tab | never counts — not an agent |
| `workspace` | never — it runs no agent |
| `tool` (tddy-coder) | never — see [Known limitations](#known-limitations) |

- A main that exited while an agent tab still runs leaves the session **active**.
- A session whose agent tabs have all closed, or that has only shell tabs, is **inactive**.
- A tmux-run agent whose client ended stays live while the daemon's detach watcher holds its tmux
  session, and for the moment between the client's end and the watcher taking over: a listing never
  finds the session held by neither.
- A sandboxed session's entry outlives its runner until resume, relaunch or delete replaces it, so
  its liveness is read off the runner process the entry owns, never off the entry's presence or the
  recorded pid.

## A daemon restart

A restarted daemon holds nothing: every session lists **inactive**, whatever its `.session.yaml`
says, until it is started, resumed or re-attached. A tmux-run agent that outlived the daemon lists
inactive until its main terminal is re-attached (opening its terminal, or Resume) and then lists
active. Each re-attach attempt announces how it ended — re-attached, already attached, or skipped
for a named reason — on the terminal manager's `reattach_finished()` stream and in the log under
the `tddy_cli_sessions::reattach` target.

## What acts on held processes

- **Delete** stops every terminal the session has (main and tabs) and its sandbox, then removes the
  directory. It never signals the recorded pid.
- **`SignalSession` and the project-home `TerminateSession`** signal the session's held main agent: its
  main terminal's process, or its sandbox runner. With none held they are refused
  (`FAILED_PRECONDITION`, `session has no running agent`) and nothing is signalled.
- **`EnsureProjectHomeSession`** answers `running` only for a main the daemon holds; a main whose
  recorded pid is alive but whose terminal nobody serves is **resumed**.
- **The project-home idle reaper** considers only live sessions and, after stopping one, waits for it
  to leave the live set — not for a pid to exit.

## Who reads it

Every consumer of `is_active` takes the live set from the daemon's terminal and sandbox managers: the
web drawer's `ListSessions`, `ListProjectSessions` and the project soft-delete refusal, branch
ownership (an *active* owner refuses a start — see [Session branch conflict](session-branch-conflict.md)),
project-home ensure and the idle reaper, the project-home session tools and git operations, the
PR-status tools, the remote-git pack resolver (which prefers the session the daemon holds a live
agent for), and Telegram's list, branch-owner scan and delete.

## Known limitations

- A `tool` (tddy-coder) session has no process the daemon holds — its child lives in the supervisor,
  the spawn worker or a detached wait thread — so a running tool session lists **inactive**, and
  delete and `SignalSession` still reach it through its recorded pid, the only handle there is. The
  recycled-pid hazard remains for that kind alone.
- A project-home main that outlived a daemon restart is held by nothing, so a replace cannot stop it
  until it is re-attached.

## Related

[Terminal sessions](terminal-sessions.md) · [Project-home auto-stop](project-home-auto-stop.md) ·
[Session branch conflict](session-branch-conflict.md) · [Claude CLI session](claude-cli-session.md) ·
[Inactive session activities](../web/inactive-session-activities.md) ·
[Session terminal tabs](../web/session-terminal-tabs.md#stopped-sessions)
