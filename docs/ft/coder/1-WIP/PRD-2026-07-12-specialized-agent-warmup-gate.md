# PRD — Specialized-agent warm-up gate on sandbox session start

- **Date:** 2026-07-12
- **PRD type:** Modification (behavioral change to existing session-start flow)
- **Product area:** `coder` (specialized subagents)

## Affected features

- [specialized-subagents.md](../specialized-subagents.md) — adds a start-time readiness gate for the resolved agent defs.
- [managed-codebase-subagents.md](../managed-codebase-subagents.md) — subagent HTTP endpoints (Ollama/SGLang) are what gets warmed.
- `tddy-sandbox-app` (macOS in-process Seatbelt path) and `tddy-daemon` (Linux cgroups path) session-start flows.

## Summary

When a sandbox session is started with one or more specialized subagents wired in (e.g. `fastcontext`
pointed at a local Ollama server), those agents' model endpoints are currently contacted **lazily** —
only when the main agent first issues a `subagent_prompt`. If the model is not yet resident (Ollama
cold start) or the endpoint is briefly unreachable, that first real call stalls or fails mid-session,
with no clear signal to the user about why.

This change adds a **warm-up gate**: before the in-jail agent CLI (`claude` / `agent`) is started, the
session proactively "wakes up" every resolved specialized agent by issuing a minimal chat-completion
against its endpoint, and **waits** until each responds successfully. Session **creation/resume fails
hard** if any agent never becomes ready within a bounded budget — no fallback to starting the session
anyway. `tddy-sandbox-app` emits clear, human-readable log output for each step (waking / retrying /
ready / failed).

## Background: what a 502 from Ollama means

Per Ollama's [API error documentation](https://docs.ollama.com/api/errors), Ollama returns
`502 Bad Gateway` specifically **"when a cloud model cannot be reached"** — i.e. when Ollama is
proxying to a hosted/cloud model and the upstream is unreachable. A `502` also commonly originates from
a **reverse proxy / gateway sitting in front of Ollama** (see [ollama#5437](https://github.com/ollama/ollama/issues/5437),
where a network-translation layer returned 502 over an unstable backend).

Crucially, for a **local** model, Ollama does **not** return 502 for "model not loaded yet." A cold
model load is a **synchronous, blocking `200`** that simply takes ~5–30s. Therefore:

- **502 ≠ "needs waking."** It is an upstream-reachability failure and must be treated as a **retryable
  transient**, not the readiness signal.
- **Explicit waking is still worthwhile**, but not because of 502 — it pays the cold-start cost up front
  and confirms the model actually answers, instead of letting the first real subagent call stall.

The warm-up therefore uses a **generic chat-completion probe** (works identically for Ollama and the
default SGLang `:30000` endpoint): a `200` means ready; connection errors / timeouts / `429` / `5xx`
(including `502`/`503`/`504`) are retried until the deadline; a definitive `4xx` such as `404`
(model not found) fails fast.

## Proposed changes

### What's changing

1. **New `tddy-discovery::warmup` module** — a backend-agnostic readiness primitive:
   - `warm_up_agents(defs, opts)` warms every resolved `SpecializedAgentDef` and returns `Ok(())` only
     when **all** are ready; otherwise a typed error naming the first agent that failed (agent name,
     `base_url`, `model`, and the last observed error).
   - Readiness probe: `POST {base_url}/v1/chat/completions` with the def's `model`, a one-token
     `max_tokens`, `temperature: 0`, `stream: false`. `200` ⇒ ready.
   - Retry policy: connection refused/reset, request timeout, `429`, and `5xx` (incl. `502`) are
     transient and retried on an interval until the total budget elapses; other `4xx` fail fast.
   - Timing is injected (`WarmupOptions { timeout, retry_interval, request_timeout }`) so tests run in
     milliseconds; production default budget is **120s** (matching the existing sandbox-ready timeout).
   - Emits `log` output (target `tddy_discovery::warmup`, `info`/`warn`) at each step.

2. **`tddy-sandbox-app` (macOS `run_macos`)** — after resolving `specialized_defs` and **before**
   `spawn_claude_sandbox`, call the warm-up gate. On failure, print a clear error and abort (non-zero
   exit); the agent CLI is never started. Prints a headline status line so the step is visible by
   default (env_logger is at `info`).

3. **`tddy-daemon`** — in `start_sandboxed_claude_cli_session` and `start_sandboxed_cursor_cli_session`,
   after `resolve_specialized_agent_defs(...)` and **before** spawning the jail, run the warm-up gate.
   On failure return `Status::failed_precondition(...)` so `StartSession` (and resume, which reuses the
   start path) fails with an actionable message. No session dir/jail is left running.

### What's staying the same

- Agent resolution (`resolve_agent_defs` / `resolve_session_agents`), the `SpecializedAgentDef` schema,
  `TDDY_SUBAGENT*` env wiring, and the lazy per-prompt subagent loop are all unchanged.
- A session with **no** specialized agents warms up nothing (immediate no-op) and starts exactly as
  today.
- The subagent request shape in `openai.rs` is untouched — the probe is a separate minimal request.

## Impact analysis

### Technical

- No new external dependencies: `reqwest`, `tokio`, and `wiremock` (dev) already exist in
  `tddy-discovery`, which both `tddy-sandbox-app` and `tddy-daemon` already depend on.
- Both the macOS host and the Linux daemon reach the agent `base_url` (e.g. `localhost:11434`) directly
  on their own host — the same host whose egress relay serves the jail's subagent calls — so warming up
  from the session-owning process hits the same endpoint the subagent will later use.
- Session start becomes slower by exactly the cold-start cost (which the first subagent call would have
  paid anyway), plus one tiny probe round-trip when already warm.

### User

- A misconfigured or down agent endpoint now fails the session **at creation** with a clear message
  ("specialized agent 'fastcontext' at http://localhost:11434 (model …) did not become ready within
  120s: connection refused"), instead of a confusing mid-session hang on the first subagent call.
- Users see explicit progress ("waking … / ready") instead of a silent pause.
- Trade-off: session start blocks on cold model loads. This is intentional and consistent with the
  "await them to be running, only then start claude" requirement.

## Implementation plan

1. Add `tddy-discovery::warmup` with unit + wiremock-backed tests (the behavioral contract).
2. Wire the gate into `tddy-sandbox-app::run_macos` (macOS) with visible log output.
3. Wire the gate into the daemon's sandboxed claude-cli and cursor-cli start paths.
4. Update `specialized-subagents.md` and changelogs.

## Acceptance criteria

- [ ] AC1 — `warm_up_agents` returns `Ok(())` once each agent's endpoint answers a chat-completion probe
  with `200`, issuing the probe to `{base_url}/v1/chat/completions` with the def's `model`.
- [ ] AC2 — A `502` (then `200`) is treated as transient: the probe retries and the agent is reported
  ready once it answers `200`.
- [ ] AC3 — Connection-refused (server not up yet), then `200`, is likewise retried to success.
- [ ] AC4 — When an endpoint never answers `200` within the budget, `warm_up_agents` returns an error
  naming the agent, its `base_url`, its `model`, and the last observed failure.
- [ ] AC5 — With multiple agents, warm-up fails if **any** one never becomes ready, and the error names
  the failing agent.
- [ ] AC6 — Empty def set ⇒ immediate `Ok(())` with **no** HTTP request issued.
- [ ] AC7 — A definitive `404` (model not found) fails fast without exhausting the full budget.
- [ ] AC8 — `tddy-sandbox-app` (macOS) runs the gate before spawning the agent CLI and aborts session
  creation on warm-up failure; it emits visible log output for waking/ready/failure.
- [ ] AC9 — The daemon's sandboxed claude-cli and cursor-cli start paths run the gate after resolving
  defs and before spawning the jail, returning `failed_precondition` on warm-up failure (covering
  resume, which reuses the start path).

## References

- [specialized-subagents.md](../specialized-subagents.md)
- [managed-codebase-subagents.md](../managed-codebase-subagents.md)
- Ollama API errors: https://docs.ollama.com/api/errors
- ollama#5437 (502 from fronting proxy): https://github.com/ollama/ollama/issues/5437
