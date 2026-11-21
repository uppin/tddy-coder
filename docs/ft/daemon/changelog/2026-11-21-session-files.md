# 2026-11-21 — An orchestrator writes session files and attaches them to the sessions it starts

`#orchestra 3/9` ([#981](https://github.com/uppin/tddy-coder/pull/981)).

- **`WriteSessionFile` and `ListSessionFiles`** are daemon exec tools for orchestrator roles
  (`project-home` main and `orchestrator`). A write goes into the caller's own session directory
  (`artifacts/files/<name>`) and nowhere else; a worker session is refused.
- **Names are one flat file name; content is at most 1 MiB.** A bad name or oversize content is refused
  and nothing is written.
- **Each answer carries the `StartSession` attachment** for the file, so a brief or plan reaches a child
  as an attachment instead of being crammed into its first message.
- Served on the host that holds the orchestrator; no peer forwarding yet.

See [Session attachments § Session files an orchestrator writes](../coder/session-attachments.md#session-files-an-orchestrator-writes).
