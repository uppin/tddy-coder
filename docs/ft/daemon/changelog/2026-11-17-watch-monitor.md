# 2026-11-17 — An orchestrator streams its watches to a native monitor

An orchestrator can take its watch events where its agent already listens: `tddy-tools monitor`
streams what its watches match as one compact JSON object per stdout line, for Claude Code's
`Monitor` tool to turn into notifications — mid-turn or idle, with no paste and no busy/idle
guess. Without `--watch` the monitor creates a watch (`--started-by-me`, `--kinds`, `--ttl`,
`--session`); with `--watch <id>` it attaches to an existing one and resumes from its last
acknowledged event, so re-arming after `Monitor`'s 30-minute expiry costs nothing. `--full` adds
the log record to each event line; by default a line is compact, because every line becomes a
message in the agent's conversation. A `permission` or `question` event carries the prompt's
`request_id`, so the agent can answer it directly.

A streamed watch is delivered to the monitor only: while a stream runs, and for 15 seconds after
each call, its events are neither attached to the orchestrator's tool answers nor pasted by the
idle wake. Each call acknowledges what was printed, so a monitor killed between receiving a batch
and printing it gets that batch again; the only duplicate possible is a printed-but-unacknowledged
one, recognisable by `seq`. When the monitor stops — expired, stopped, or the daemon restarted —
everything after the last acknowledged event returns to answers and the wake as if no monitor had
run. A second monitor on the watch's own orchestrator takes the stream over at once; the
superseded monitor is refused from its next call and exits non-zero. The monitor exits 0 once
every streamed watch has ended, and non-zero after an `error` line on any refusal, transport loss
or per-call deadline lapse. See
[Project concept § Watching sessions](../project-concept.md#watching-sessions).

A manual confirmation remains for the operator: under Claude Code's `Monitor` on a real session,
an idle orchestrator is told of a watched permission prompt and answers it.
