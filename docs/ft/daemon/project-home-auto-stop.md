# Project-home auto-stop

**Status:** ✅ Shipped (`#project-home` 6/7, PR #701)

## Summary

A running **project-home** session (role `project-home`, see [Project concept](project-concept.md)) is
stopped automatically once its agent has been idle for `project_home.idle_stop_secs`. It keeps its
agent conversation, so the next open of its pinned entry (`EnsureProjectHomeSession`) **resumes the
same conversation**. No other session is ever auto-stopped.

## Idle

- **Busy** while the hook-reported `activity_status` is `Running` (a turn in flight). A busy session is
  never stopped, however long the turn.
- Otherwise **idle since** the session metadata's `updated_at`, which every status report bumps — the
  end of its last turn or the input that started one. New input therefore resets the clock.
- A session whose `updated_at` does not parse, or that has no recorded pid, is left alone.

## Configuration

```yaml
project_home:
  idle_stop_secs: 600   # default (10 minutes); 0 disables auto-stop
```

Absent section = the default.

## Behaviour

- A daemon task checks every **30 s** (the granularity of the stop) the listing's active sessions with
  the `project-home` role, in every mapped user's sessions directory.
- The stop is the one an explicit termination makes: the tracked terminal is cancelled (SIGTERM, then
  SIGKILL to its child) and forgotten. `.session.yaml` is not written, so resume has what it needs.
- The reaper waits up to 5 s for the process to exit before counting the session as stopped.

## Known limitations

- A session whose process is alive but **not tracked** by the daemon's CLI session manager (e.g. one
  started by a previous daemon process) cannot be stopped by this path; each tick logs a warning after
  the 5 s wait. Backlog: [docs/dev/todo/2026-10-23-project-home-auto-stop-untracked-processes.md](../../dev/todo/2026-10-23-project-home-auto-stop-untracked-processes.md).

## Related

- [Project concept](project-concept.md) · [Claude CLI session](claude-cli-session.md) · [Agent session status](agent-session-status.md)
