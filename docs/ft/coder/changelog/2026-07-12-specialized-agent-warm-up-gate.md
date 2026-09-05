# 2026-07-12 — Specialized-agent warm-up gate

- Starting a sandbox session with specialized agents wired in now **warms them up first**: each resolved agent's model endpoint is woken and awaited before the in-jail agent CLI starts, so a cold **Ollama** model or unreachable endpoint surfaces at session creation instead of stalling the main agent's first `subagent_prompt`.
- **No fallback:** session creation/resume fails hard (a clear error naming the agent, endpoint, and model) if any agent never becomes ready within a 120s budget.
- Backend-agnostic readiness probe (`/v1/chat/completions`, one token) works for Ollama and the SGLang `:30000` default alike; `502`/`5xx`/`429`/connection-errors are retried as transients, a `404` (model not found) fails fast.
- On the 502 question: per Ollama's API docs a `502` means *a cloud model cannot be reached* (or a fronting proxy failed), **not** "model unloaded" — a local cold-load is a blocking `200` — so `502` is a retryable transient, not the readiness signal.
- Gates both the `tddy-sandbox-app` macOS path (visible log output, Ctrl-C-interruptible) and the daemon's sandboxed claude-cli + cursor-cli start paths (`failed_precondition`; covers resume).
- Feature: [specialized-subagents.md § Start-time warm-up gate](../specialized-subagents.md#start-time-warm-up-gate)
