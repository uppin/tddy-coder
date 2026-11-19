# 2026-11-19 — `--binary` reaches a target, and a stop kills only its own session's agent

**Type:** Fix

`--binary` sets the tool's command as well as its binary, so a target launch runs it. `stop_session`
kills by `<worktree>/.tddy/agents/<session id>.pid` and removes it in the same command. A Coder
session's `exit` is the agent's status, not the CLI's 1. See [architecture.md](../architecture.md).
Full record: [`docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md`](../../../../docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md).
