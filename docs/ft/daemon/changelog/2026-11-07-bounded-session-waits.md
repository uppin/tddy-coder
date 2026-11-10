# 2026-11-07 — A session start that cannot finish fails, naming its step

`#agent-tools` 23/28 ([#831](https://github.com/uppin/tddy-coder/pull/831)).

- **Starts have a deadline.** `session_start_timeout_secs` in `daemon.yaml` (default 600 s) bounds
  every start — web, peer and Telegram. An overrun is cancelled, what it created is removed, and the
  caller gets `DEADLINE_EXCEEDED` naming the step it was on. See
  [session-agent-roster.md § A start has a deadline](../session-agent-roster.md#a-start-has-a-deadline).
- **Supervisor calls cannot hang the daemon.** Each call fails after 60 s, a silent clone after 300 s,
  naming the method. See [tddy-supervisor.md](../../supervisor/tddy-supervisor.md).
- **Telegram** no longer has a bound of its own; it reports the daemon's.
