# 2026-11-22 — An orchestrator's session files reach the sessions it starts on other hosts

- `WriteSessionFile` / `ListSessionFiles` answer an `attachment` that names the holding daemon, so a session started on any host receives the file.
- A session-file call landing on a daemon that does not hold the orchestrator is served by the one that does.

Feature: [session-attachments.md § Session files an orchestrator writes](../../coder/session-attachments.md#session-files-an-orchestrator-writes)
