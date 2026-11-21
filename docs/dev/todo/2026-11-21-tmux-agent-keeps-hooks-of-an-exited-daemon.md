# 2026-11-21 — An agent kept alive in tmux across a daemon restart keeps the hooks of the exited daemon

**Category:** Deferred from "A claude-cli session's hooks are a file of its own, rendered on every launch" (PR #977)
**Source:** `packages/tddy-daemon-kernel/src/agent_tool_socket.rs` (`tddy-agent-tools-<pid>-<n>.sock`),
`packages/tddy-agent-launch/src/claude_cli_spawn_steps.rs` (`resumed_hooks`)

## What happens

Claude Code reads its hooks once, when it starts. Every launch writes a claude-cli session's hooks
file again, naming the running daemon's agent-tool socket. An agent tab that runs in tmux outlives a
daemon restart and is re-attached at startup (`#session-liveness` 2/8), but it is not relaunched, so
it keeps the hooks it loaded before the restart. The socket those hooks name carries the exited
daemon's pid, so every hook fails quietly (`tddy-tools session-hook` is fail-quiet). The session
records no activity and no `/clear` until it is resumed.

## What would close it

Pick one, or both:

- A socket path that is stable across daemon restarts, per OS user. The hooks would then stay valid
  for any live agent.
- Re-attaching a tmux agent re-renders its hooks file, and the hook command reads the socket from a
  file at run time.

Pin it with an acceptance test: a tmux claude stub survives a daemon restart, fires a hook, and the
session records it.
