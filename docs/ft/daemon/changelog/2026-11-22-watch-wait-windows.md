# 2026-11-22 — Session watches no longer interrupt an orchestrator; it waits for them in a window

- A watch wakes its idle orchestrator with a `[tddy watch]` turn only when created or renewed with `wake_when_idle` (default off) — injected turns cut across what the operator was typing. Its events wait for `WaitSessionWatchEvents`, a monitor, or the next tool answer.
- `WaitSessionWatchEvents` waits in a window: it returns once it holds `max_events` (default 1), `settle_ms` after its first event, or when `timeout_ms` passes (default a minute, at most 30 minutes; a call held past ~2 minutes runs in the background in Claude Code).
- One call can pick `kinds` and `session_ids`; what it leaves stays pending, and nothing acknowledged is handed twice.
- An orchestrator whose agent takes no terminal turn (Codex, ACP) may now hold a watch that does not wake.

Feature: [project-concept.md § Watching sessions](../project-concept.md#watching-sessions)
