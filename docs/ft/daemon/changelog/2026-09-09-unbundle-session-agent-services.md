# 2026-09-09 — The session-agent and activity services

A session's agents, and what they are doing, are services of their own.

**`session_agents.SessionAgentService`** carries the agent roster: attaching, detaching, listing and
subscribing to the set of specialized agents on a session, plus the conversations held with them —
opening one, prompting it, cancelling a turn in flight, and the two reports a peer daemon or a jailed
agent pushes back about a checkout's readiness and a conversation's state. Nine methods, served by
the new `tddy-session-agents` crate.

**`activity.ActivityService`** carries what an agent is doing and the transcript of both it and the
session: the two hook reports (`ReportSessionStatus`, `ReportAgentActivity`), the live activity
stream, the session-notification feed behind the drawer's indicators, the worktree delta stream a
mirror follows, and the three ACP transcript-replay methods. Eight methods, served by the new
`tddy-session-activity` crate — and, for four of them, by a `tddy-coder` session's own participant.

`connection.ConnectionService` keeps session lifecycle, projects, tools and the agent catalog — **33
methods**, down from 50.

## For operators

**A `tddy-web` bundle and a daemon must be upgraded together.** Seventeen coordinates moved. An older
bundle cannot attach an agent, open a conversation, or render the activity, notification and replay
panes against a newer daemon.

**A newer `tddy-tools` must be installed alongside a newer daemon, and this one fails silently.**
`tddy-tools session-hook` is baked into every Claude and Cursor CLI session's hook configuration, and
it posts `ReportSessionStatus` and `ReportAgentActivity`. Both moved. The hook swallows every error
and exits 0 by contract, so a mismatched pair does not log, alert or fail a session — it just stops
recording session status, stops raising Telegram attention alerts, and stops filling the activity
pane. Sessions whose hooks were written by an older daemon keep pointing at the old path until they
are restarted.

**A session's first activity delta is now numbered 1.** It used to be 0, which is also the wire's
"no tick yet" value, so a mirror could not tell *the first change a session made* from *no change
measured yet* — and its content only reached the mirror at the next reconcile. `tddy-session-sync` is
the only consumer and was migrated in the same release. A mirror binary older than the daemon it
follows will treat the first delta as absent, exactly as it did before; a newer one against an older
daemon does the same. The two only disagree about that one delta.

**What an in-jail agent may do is unchanged.** The sandbox relay allowlist — the list of operations
an agent running inside a jail may ask its host to perform — permits exactly the same five operations
as before. Only the service name each entry carries changed, and the runner and the daemon now read
that list from one shared declaration so they cannot drift apart.

**Nothing was dropped from the daemon's local Unix socket.** Both new services are mounted there
alongside the four that were already present, because a family that used to be reachable on that
socket and quietly stops being reachable is a capability removal with no announcement and no error
worth reading.

## What did not change

Every roster, conversation, activity, notification and replay behaviour. This is a relocation: the
same handlers, the same refusals, the same ordering. The two deliberate exceptions are the delta tick
numbering above, and `tddy-tools session-hook`'s URL, which had to move with the methods it posts to.

`ListSessions` and the session-status badge it carries stay on `connection.ConnectionService` —
including the inference that fills them, which travelled into `tddy-session-agents` with the modules
it shares a truncation rule with but is still read only by `ListSessions`.

## Known gaps recorded with this change

- The acceptance test that drives a **real jail** through the moved conversation path does not run:
  it is macOS-gated as an inner attribute, so Linux CI compiles it to an empty binary, and on macOS
  it fails in setup on a stdio-bridge defect that predates this change.
- **Nothing verifies that the moved ACP replay output matches what the old coordinate produced.** For
  a pure relocation that is the check that matters most, and it is absent.
- `tddy-session-activity` ships with no tests of its own; everything covering it lives in the
  daemon's suites.

All three are in [`docs/dev/todo/`](../../../dev/todo/), dated 2026-09-12.

## References

- Full engineering record: [docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../dev/changesets/2026-09-09-unbundle-session-agent-services.md)
- [session-agent-roster.md](../session-agent-roster.md) — the roster feature
- [agent-session-status.md](../agent-session-status.md) — the session's own agent status
- [session-notifications.md](../session-notifications.md) — the notification feature
- [remote-codebase-mode.md](../remote-codebase-mode.md) — the in-jail relay allowlist
- [agent-activity-pane.md](../../web/agent-activity-pane.md) — the browser's reader
- [specialized-subagents.md](../../coder/specialized-subagents.md) — where agent defs come from
- Node 6's entry, for the lockstep argument: [2026-09-11-unbundle-session-io-services.md](./2026-09-11-unbundle-session-io-services.md)
