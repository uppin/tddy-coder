# 2026-07-19 — Workflows spawn a child conversation via `spawn_conversation`

- A managed workflow agent can call the new `spawn_conversation` MCP tool to start a fresh interactive claude-cli conversation on a newly created git worktree, tagged as a child of the calling (orchestrator) session — reusing the same spawn path as the PR-stack `spawn-child`, which is left untouched. See [spawn-conversation.md](../spawn-conversation.md).
- The **grill-me** Create-plan prompt is the first consumer: after writing the plan brief it commits `plans/<slug>.md` and calls `spawn_conversation` with the brief path + a branch, handing the plan to a new implementation agent.
- Resolution depends on session type: claude-cli managed sessions run the daemon-side handler in-process; cursor/grill-me **tool** sessions relay the tool call to the daemon over a new auth-free **stdio reverse-RPC** channel (the OS pipe is bound 1:1 to the spawned child).
- Deferred (not a blocker): the daemon→coder presenter-intent path still uses localhost `--grpc`; migrating it onto the stdio client and wiring the resume + remote/multi-host spawn paths is tracked as `TODO(stdio-relay)`.
