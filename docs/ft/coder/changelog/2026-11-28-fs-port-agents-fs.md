# 2026-11-28 — Agent launch and backends go through the Fs port

The production disk I/O of `tddy-agent-launch`, `tddy-agent-backend`,
`tddy-coding-subagent`, `tddy-agent-skills` and `tddy-coding-agent-specs` goes through the
`tddy-fs` port (refactor, no behaviour change): launches run behind `LaunchSessions.fs`,
backends behind held handles, and free functions behind `<fn>_on` seams with `DiskFs` only at
composition roots — service constructors and the daemon's wiring. No port surface was added.
