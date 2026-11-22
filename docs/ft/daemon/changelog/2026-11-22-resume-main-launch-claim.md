# 2026-11-22 — A resume refuses a duplicate or running launch of a session's agent

A second **Resume** of a CLI-agent session while one is still in flight, or a Resume of a session whose
agent is still running, is refused with a `FAILED_PRECONDITION` naming why, instead of launching a
second agent on the same worktree — which, on a Coder target where a resume takes minutes, a double
click used to do, leaving one agent that Stop could not reach. The daemon logs when a resume begins
and when its agent is up. See
[Claude CLI session § Resume after process exit](../claude-cli-session.md#resume-after-process-exit).
