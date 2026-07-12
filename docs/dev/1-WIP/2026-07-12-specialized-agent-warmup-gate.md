# Changeset: Specialized-agent warm-up gate on sandbox session start

- **Date:** 2026-07-12
- **Status:** 🚧 In Progress
- **Type:** Feature (behavioral gate) + new library module
- **PRD:** [docs/ft/coder/1-WIP/PRD-2026-07-12-specialized-agent-warmup-gate.md](../../ft/coder/1-WIP/PRD-2026-07-12-specialized-agent-warmup-gate.md)

## Affected areas

- `packages/tddy-discovery/src/` — new `warmup` module (readiness primitive), `lib.rs` export.
- `packages/tddy-sandbox-app/src/main.rs` — macOS `run_macos` start gate.
- `packages/tddy-daemon/src/connection_service.rs` — sandboxed claude-cli + cursor-cli start gate.
- Docs: `docs/ft/coder/specialized-subagents.md`, `docs/ft/coder/changelog.md`.

## Related feature documentation

- [specialized-subagents.md](../../ft/coder/specialized-subagents.md)
- [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)

## Summary

Add a **warm-up gate** that runs when a sandbox session starts with specialized subagents wired in:
before the in-jail agent CLI is launched, each resolved `SpecializedAgentDef`'s model endpoint is
proactively woken (a minimal chat-completion probe) and the session **waits** until every agent answers
`200`. Session creation/resume **fails hard** if any agent never becomes ready within a bounded budget
(default 120s). `tddy-sandbox-app` prints clear log output at each step.

## Background

See the PRD for the full analysis of Ollama's `502` semantics. Short version: `502` is an
upstream/proxy reachability failure, **not** a "model unloaded" signal (local cold-start is a blocking
`200`), so the probe treats `502`/`5xx`/`429`/connection-errors/timeouts as **retryable transients** and
a definitive `404` as a **fast failure**.

## Technical changes

### State A (current)

- Specialized agents are resolved at session start (`resolve_session_agents` in the app;
  `resolve_specialized_agent_defs` in the daemon), serialized into `TDDY_SUBAGENT*` env, and wired into
  the in-jail `tddy-tools --mcp`. Their model endpoints are **never contacted at start**.
- The endpoint is first hit **lazily**, on the main agent's first `subagent_prompt`
  (`tddy_discovery::subagent::SpecializedSubagentSession` → `openai::OpenAiClient::complete` →
  `POST {base_url}/v1/chat/completions`). A cold or unreachable endpoint stalls or fails that first
  call mid-session.
- `openai::OpenAiClient::complete` collapses any non-2xx into a `String` error, discarding the status
  code — so it cannot classify transient vs permanent failures.

### State B (target)

- A new `tddy_discovery::warmup` module provides a backend-agnostic readiness gate that both session
  owners (app on macOS, daemon on Linux) call after resolving defs and before spawning the jail/CLI.
- Warm-up issues its own minimal probe (independent of `OpenAiClient`) so it can read the raw HTTP
  status and classify retryable-vs-fatal.
- Session start **blocks** on all agents being ready and **fails** (non-zero exit / `failed_precondition`
  Status) if any is not ready within the budget. No fallback to starting anyway.

### Delta

#### `packages/tddy-discovery/src/warmup.rs` (new)

```rust
pub struct WarmupOptions {
    pub timeout: Duration,          // total budget per agent (prod default 120s)
    pub retry_interval: Duration,   // wait between transient retries (prod default ~1s)
    pub request_timeout: Duration,  // per-probe HTTP timeout (prod default: remaining budget)
}
impl Default for WarmupOptions { /* 120s / 1s / 120s */ }

/// One agent's warm-up failure — carries everything needed for an actionable message.
pub struct AgentWarmupError {
    pub agent: String,
    pub base_url: String,
    pub model: String,
    pub last_error: String,
}
impl std::fmt::Display / std::error::Error for AgentWarmupError

/// Warm up every def; Ok only when ALL are ready. Errors on the first agent that never becomes ready.
pub async fn warm_up_agents(
    defs: &[SpecializedAgentDef],
    opts: &WarmupOptions,
) -> Result<(), AgentWarmupError>;
```

- Probe: `POST {base_url}/v1/chat/completions` body
  `{model, messages:[{role:"user",content:"ping"}], max_tokens:1, temperature:0, stream:false}`.
- Classification:
  - `2xx` ⇒ ready.
  - connection refused/reset, request timeout, `408`, `429`, `5xx` (incl. `502`/`503`/`504`) ⇒ transient,
    sleep `retry_interval`, retry until total `timeout` elapses.
  - any other status (e.g. `400`/`401`/`403`/`404`) ⇒ fatal, fail immediately with the status + body.
- Logging (target `tddy_discovery::warmup`): `info` "warming up N specialized agent(s): …"; `info`
  "waking '<name>' (model <model>) at <base_url> …"; `warn` on each transient retry with reason +
  elapsed/budget; `info` "'<name>' is ready (<elapsed>)".
- Empty `defs` ⇒ `Ok(())` immediately, no HTTP.

#### `packages/tddy-discovery/src/lib.rs`

- `pub mod warmup;`

#### `packages/tddy-sandbox-app/src/main.rs` (`run_macos`)

- After `let specialized_defs = config::resolve_session_agents(...)?;` (currently ~line 358) and
  **before** the `spawn_claude_sandbox(...)` `tokio::select!` (~line 389):
  - If `!specialized_defs.is_empty()`, `eprintln!` a headline ("waking N specialized agent(s) before
    starting <agent_kind> …"), then `warmup::warm_up_agents(&specialized_defs, &WarmupOptions::default())
    .await`.
  - On `Err(e)`: `eprintln!` a clear failure and `return Err(anyhow!(...))` — the agent CLI is never
    spawned. (Runs inside the existing `ctrl_c` race is unnecessary; the gate is fast to interrupt via
    Ctrl-C already because it's `.await`ed before spawn — but keep it interruptible by wrapping in the
    same `tokio::select!` against `ctrl_c` as the spawn.)

#### `packages/tddy-daemon/src/connection_service.rs`

- In `start_sandboxed_claude_cli_session` (after `let specialized_defs = self.resolve_specialized_agent_defs(...)?;`, ~line 1084)
  and in `start_sandboxed_cursor_cli_session` (~line 1513), **before** the jail spawn:
  - `warmup::warm_up_agents(&specialized_defs, &WarmupOptions::default()).await
      .map_err(|e| Status::failed_precondition(e.to_string()))?;`
- Resume reuses the start path, so it is gated automatically.

## Implementation milestones

- [x] `warmup` module compiles with `warm_up_agents` + `WarmupOptions` + `AgentWarmupError` public API.
- [x] All `warmup` unit/acceptance tests pass (wiremock-backed) — 8 acceptance + 5 unit, all green (0.53s).
- [x] `tddy-sandbox-app` macOS path gates on warm-up before spawn, with visible log output
  (`main.rs` `run_macos` ~369–389; `ctrl_c`-interruptible; `Err` ⇒ abort, CLI never spawned).
- [x] Daemon claude-cli + cursor-cli sandboxed paths gate on warm-up before jail spawn
  (`connection_service.rs` ~1090 and ~1530; `Err` ⇒ `Status::failed_precondition`; resume reuses
  the start path).
- [ ] `docs/ft/coder/specialized-subagents.md` + changelog updated (pr-wrap phase).
- [x] Targeted `cargo clippy -- -D warnings` clean for `tddy-discovery` + the two touched call sites.

## Testing plan

### Test level

The behavior-defining seam is `warm_up_agents` against an HTTP endpoint. `tddy-discovery` already has
`wiremock` + `tokio` dev-deps and uses them for `openai.rs`. So the **acceptance + unit tests target
`warm_up_agents` with a wiremock server** — real HTTP, deterministic, millisecond-fast via injected
`WarmupOptions`. This is strictly better than mocking the daemon/app spawn machinery (which would need a
Seatbelt jail / running daemon and test nothing about the actual readiness contract).

The app/daemon integration is a two-line call-site wiring each; it is covered by the module contract plus
a pure unit assertion where a seam exists (see below). Full end-to-end spawn is **not** unit-testable and
is out of scope for automated tests here (consistent with the existing spawn code, which is exercised
manually).

### Testing options considered

| Option | Verdict |
|--------|---------|
| wiremock against `warm_up_agents` | **Chosen** — exercises the real probe, retry, and failure contract. |
| Mock the whole daemon `StartSession` | Rejected — needs a running daemon; tests wiring, not readiness. |
| Real Ollama in CI | Rejected — heavy, non-deterministic, not available in CI. |

### Coverage requirements

Every PRD acceptance criterion AC1–AC7 maps to a `warm_up_agents` test. AC8/AC9 (app/daemon wiring) are
asserted structurally by the shared contract; no automated spawn test.

### Acceptance tests

**File:** `packages/tddy-discovery/tests/specialized_agent_warmup_acceptance.rs` (new)

Fluent Given/When/Then, one behavior per test, exact assertions, injected sub-second `WarmupOptions`.
Helpers: `a_warmup_agent(name, base_url)` builder over `SpecializedAgentDef`; `fast_warmup_options()`
(e.g. 2s budget, 20ms retry) with a comment justifying the values; `assert_warmup_error(result)` domain
assertion exposing `.for_agent(name)` / `.mentions(fragment)`.

1. `reports_an_agent_ready_once_its_endpoint_answers_a_chat_completion` — wiremock returns `200` on
   `POST /v1/chat/completions`; `warm_up_agents` returns `Ok(())`; the mock recorded exactly one request
   carrying the def's `model`. *(AC1)*
2. `retries_a_502_until_the_endpoint_becomes_ready` — mock returns `502` for the first request, `200`
   thereafter (wiremock `up_to_n_times`); `warm_up_agents` returns `Ok(())`. *(AC2)*
3. `retries_a_connection_refused_endpoint_until_it_comes_up` — start with the port closed / a
   `503`-then-`200` mock standing in for "not up yet"; warm-up succeeds once it answers. *(AC3)*
4. `fails_with_an_actionable_error_when_an_agent_never_becomes_ready` — mock always `502`; warm-up
   returns `Err` naming the agent, its `base_url`, and its `model`. *(AC4)*
5. `fails_if_any_one_of_several_agents_never_becomes_ready` — two agents, one ready (`200`), one always
   `502`; warm-up returns `Err` naming the failing agent only. *(AC5)*
6. `warms_up_nothing_and_makes_no_request_for_an_empty_agent_set` — empty defs; `Ok(())`; a mounted
   mock that would panic-on-hit records zero requests. *(AC6)*
7. `fails_fast_when_the_model_is_not_found` — mock returns `404`; warm-up returns `Err` well within the
   budget (assert elapsed ≪ timeout) without exhausting retries. *(AC7)*

### Red-phase unit tests

**File:** `packages/tddy-discovery/src/warmup.rs` `#[cfg(test)] mod tests` (colocated)

Pure helpers, no I/O:

8. `classifies_502_503_504_429_and_5xx_as_transient` — `classify_probe_status(code)` ⇒ `Transient` for
   502/503/504/429/500. *(rstest cases)*
9. `classifies_400_401_403_404_as_fatal` — ⇒ `Fatal`. *(rstest cases)*
10. `classifies_200_as_ready` — ⇒ `Ready`.
11. `builds_a_one_token_probe_request_body_for_the_defs_model` — the probe body serializes with the
    def's `model`, `max_tokens: 1`, `temperature: 0`, `stream: false`, one `user` message.
12. `warmup_error_display_names_agent_base_url_and_model` — `AgentWarmupError::to_string()` contains the
    agent name, `base_url`, and `model`.

## Technical debt & production readiness

- Pre-existing (not introduced here): `cargo clippy -p tddy-daemon --all-targets` reports
  `assertions-on-constants` at `connection_service.rs:6576-6577`
  (`assert!(TERMINAL_OUTPUT_FRAME_MAX_BYTES …)`, from the terminal-capture chunking work). Out of
  scope for this changeset.
- No automated test covers the app/daemon call-site wiring itself (would require a real Seatbelt
  jail / running daemon); the readiness contract is fully covered by the `warmup` module tests.

## Decisions & trade-offs

- **Generic chat-completion probe** (not Ollama-native `/api/ps`) — one code path warms Ollama and the
  SGLang `:30000` default identically; naturally triggers Ollama's blocking cold-load. Costs one
  1-token completion per agent per start (free for local backends). Approved in planning.
- **Gate both macOS app and Linux daemon paths** — chosen in planning (over macOS-only); resume is
  covered because it reuses the daemon start path.
- **502 is transient, 404 is fatal** — grounded in Ollama's documented error semantics (see PRD).
- **Injected `WarmupOptions`** — production defaults to 120s to match the existing sandbox-ready
  timeout; tests inject sub-second budgets so the suite stays within the unit/integration timeout
  budget.
- **No new dependencies** — reuses `reqwest`/`tokio`/`wiremock` already present in `tddy-discovery`.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests — verify 100% pass
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps
