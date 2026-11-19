# 2026-11-19 — `spawn_rendered_client_main` for a transport client's main

**Type:** Fix

A main whose process is a transport's client to an agent on a target names how the agent's status is
read from the client's output (`tddy_pty::ExitCodeInOutput`); `spawn_tool_on` takes it. See
[module-layout.md](../module-layout.md). Full record: [`docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md`](../../../../docs/dev/changesets/2026-11-19-remote-agent-pid-per-session.md).
