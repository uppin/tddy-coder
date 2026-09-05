# 2026-08-13 — A resumed session's subagents are warmed up too, and the warm-up budget is yours to set

- **Resuming a sandboxed session now runs the same specialized-agent warm-up gate a fresh start does.** It never did: resume goes through a different path, so a resumed session could hand the agent a subagent whose first call stalled on a cold model. As at start, a failed warm-up fails the resume — the jail is not relaunched anyway.
- The docs that said resume was already covered ("resume reuses the start path") were **wrong**, not merely out of date; they now say what actually happens.
- **The warm-up budget is operator configuration**, not a constant: `agent_warmup: { timeout_secs, retry_interval_ms, request_timeout_secs }` in `daemon.yaml`, defaulting to today's 120 s / 1 s / 120 s, overridable per process by `TDDY_AGENT_WARMUP_TIMEOUT_SECS`, `TDDY_AGENT_WARMUP_RETRY_INTERVAL_MS`, `TDDY_AGENT_WARMUP_REQUEST_TIMEOUT_SECS`. A host whose endpoints are local and fast no longer waits out a budget sized for a GPU bringing a model up cold to learn one is down.
- Known gap: a **standalone** (`tddy-sandbox-app`) session keeps the built-in budget — it has its own config schema — so it and a daemon-hosted session on the same host can warm up differently.
- See [specialized-subagents.md § Start-time warm-up gate](../specialized-subagents.md#start-time-warm-up-gate).
