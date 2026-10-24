# Changeset: One ACP client driver for every ACP coding backend

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Technical Improvement
**Complexity**: 4 — one crate plus the stub and integration tests, a spawned-process seam, concurrency (cancel while a turn runs), an inherited restructure (two complexity records)
**Branch**: `feature/coagents/acp-driver`, base `master` — `#coagents` 1/8
**PR**: _assigned at wave 1_

## Initial Discovery

[2026-10-24-coagents-acp-driver-initial-discovery.md](./2026-10-24-coagents-acp-driver-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ✅ RESOLVED HERE — The three ACP backends each carry their own copy of the worker — [`2026-10-22-acp-backends-share-one-worker.md`](../todo/2026-10-22-acp-backends-share-one-worker.md)

This node *is* the fix: one generic worker parameterised by per-agent hooks. Wrap deletes the file
and removes `TODO(opencode-acp)` from `opencode_acp.rs`.

### ✅ RESOLVED HERE — `complexity-acp-run-acp-worker` — `packages/tddy-agent-backend/docs/code-issues/complexity-acp-run-acp-worker.md`

`run_acp_worker` (184 lines, nesting 10) is replaced by the driver; re-measured at wrap and closed.

### ✅ RESOLVED HERE — `complexity-codex-acp-run-codex-acp-worker` — `packages/tddy-agent-backend/docs/code-issues/complexity-codex-acp-run-codex-acp-worker.md`

Same; Codex's OAuth retry becomes a hook.

## Responsibility

This PR owns the **ACP client driver** in `tddy-agent-backend` and moving `claude-acp`, `codex-acp`
and `opencode` onto it:

- `AcpDriver` — spawns one ACP agent child (program, args, env, secret env, cwd), connects
  `ClientSideConnection` over its stdio on a dedicated thread with a `LocalSet`, initializes once,
  and kills the child on drop.
- `AcpDriver::open_session(OpenSession)` — `session/new` or `session/load`, with `cwd`, `mcp_servers`
  and an optional model applied through `session/set_model`.
- `AcpDriver::prompt(session, blocks, sink)` — one `session/prompt`, every `session/update` mapped to
  `ProgressEvent`s and to an `AcpUpdate` stream for callers that want the raw ACP shape; returns
  `AcpTurnEnd { stop_reason, text }`.
- `AcpDriver::cancel(session)` — `session/cancel`.
- `AcpDriver::list_models()`.
- `PermissionHandler` — the port the driver asks on `session/request_permission`. `AllowOnce` is the
  implementation the three workflow backends install.
- `AcpAgentHooks` — per-agent behaviour: Codex's auth-retry, OpenCode's route files, Claude's route env.
- `tddy-acp-stub` additions: `TDDY_ACP_STUB_RECORD=<path>` appends one JSON line per received
  `new_session` / `load_session` / `set_session_model` / `cancel`; a `hold_until_cancel` response
  template; a scenario `models` list advertised on `session/new`.

## Boundaries

- No `CodingAgentSession`, no roster, no `tddy-tools` change — that is n2 `acp-subagent`.
- No permission *yield* to a caller — n3 `permission-yield` installs a different `PermissionHandler`.
- No headless `-p` transport — n4 `headless-driver`.
- `CodingBackend` keeps its signature; `InvokeRequest`/`InvokeResponse` are unchanged.
- No behaviour change for workflow callers beyond the PRD's list (model honoured, cancel, kill on drop, claude-acp `load_session`).

## Dependencies

This node is the stack's root and consumes nothing from another node.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| — (root, `master`) | — | — | — |

## Draft PR contract

The first push after this plan publishes:

- `packages/tddy-agent-backend/src/backend/acp_driver.rs` (new module, re-exported from the crate
  root) with the public types and signatures listed under *Responsibility*, bodies
  `// TODO(acp-driver): implement`.
- The `tddy-acp-stub` recorder, `hold_until_cancel` and `models` (stub-side, implemented — it is test
  infrastructure the failing tests need to observe the client).
- The failing tests listed under *Acceptance Tests*.

Dependents compile against `AcpDriver`, `OpenSession`, `AcpTurnEnd`, `AcpUpdate`, `PermissionHandler`,
`PermissionRequest`, `PermissionDecision`, `McpServerSpec`.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — every test drives `tddy-acp-stub`; nothing from another node
**Concurrent with:** none (only wave-1 node)
**Blocks:** n2 `acp-subagent` (its `CodingAgentSession` drives `AcpDriver`)

Real dependency edges, as opposed to the branch line:

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-agent-backend**: [README.md](../../packages/tddy-agent-backend/README.md) — new `backend/acp_driver.rs`; `acp.rs`, `codex_acp.rs`, `opencode_acp.rs` reduced to hooks; `acp_updates.rs` folded into the driver's mapping
- **tddy-acp-stub**: request recorder, `hold_until_cancel`, advertised `models`
- **tddy-integration-tests**: `tests/acp_driver_acceptance.rs`; existing `acp_backend_acceptance.rs`, `codex_acp_backend.rs`, `opencode_acp_backend.rs` must stay green

## Related Feature Documentation

- [PRD-2026-10-24-coagents-acp-driver.md](../../ft/coder/1-WIP/PRD-2026-10-24-coagents-acp-driver.md)
- [codex-acp-backend.md](../../ft/coder/codex-acp-backend.md), [opencode-acp-backend.md](../../ft/coder/opencode-acp-backend.md)

## Summary

Replace three copies of the ACP worker with one driver that can cancel, set the model, pass MCP
servers, route permission requests to a handler and kill its child on drop.

## Background

See the PRD's capability table. The stack's next node needs all of these to run a coding agent as a
subagent; the workflow backends gain the model and cancel behaviour for free.

## Scope

- [ ] **Implementation**: `acp_driver.rs`; the three backends on it; stub additions
- [ ] **Testing**: acceptance tests below; existing ACP backend suites green
- [ ] **Package Documentation**: `tddy-agent-backend` docs § ACP driver
- [ ] **Code Quality**: two complexity records re-measured and closed

## Technical Changes

### State A (Current)

- `backend/acp.rs` (`ClaudeAcpBackend`, 382 lines): `run_acp_worker` (`:73`, 184 lines) owns a thread
  with a `LocalSet`, spawns `bunx claude-agent-acp` (`:307`), route env only, stderr to null
  (`:259-281`); an in-memory `HashMap<tddy id, acp sid>` for resume (`:121,163-177`), never
  `load_session`; prompt is `vec![request.prompt.into()]` (`:190`) — `model` and `system_prompt`
  ignored; `request_permission` answers `allow_once` (`:59-68`); `AcpCommand::Shutdown` never sent.
- `backend/codex_acp.rs` (509 lines): its own worker; system prompt merged into the prompt (`:146`);
  `load_session` resume (`:177-217`); one `codex login` retry on an auth-like error (`:185-243`,
  `codex_acp/oauth.rs`).
- `backend/opencode_acp.rs` (520 lines): its own worker already split into `ensure_initialized` /
  `run_prompt` / `list_session_models` (`:287-337`); route files → `OPENCODE_CONFIG` (`:105-165`);
  `TODO(opencode-acp)` (`:72-75`).
- `backend/acp_updates.rs`: `record_session_update` (`AgentMessageChunk`/`ToolCall`/`ToolCallUpdate`/`Plan` →
  `ProgressEvent`) and `allow_once()` — the only shared code.
- No `set_session_model`, `cancel`, `mcp_servers` or `kill_on_drop` anywhere in the crate.
- `agent-client-protocol = "=0.10.4"` (features `unstable`) — `Agent::set_session_model` exists.
- `tddy-acp-stub` scenario: `chunks`, `tool_calls`, `permission_requests` (auto-approved), `stop_reason`,
  `error`; `cancel` is a no-op; nothing is recorded.

### State B (Target)

```
ClaudeAcpBackend ─┐                         ┌─ AcpAgentHooks (route env / route files / auth retry)
CodexAcpBackend  ─┼─► AcpDriver ────────────┤
OpenCodeAcpBackend┘     │ thread + LocalSet └─ PermissionHandler (AllowOnce for workflow backends)
                        │ child (kill on drop) ── stdio ── ClientSideConnection
                        ├ open_session(OpenSession{cwd, resume, model, mcp_servers})
                        ├ prompt(session, blocks, sink) -> AcpTurnEnd{stop_reason, text}
                        ├ cancel(session)
                        └ list_models()
```

### Delta

- **tddy-agent-backend**: add `backend/acp_driver.rs` (+ submodules as needed to stay under the file
  budget); `acp_updates.rs` mapping moves under it; the three backends keep `CodingBackend` and
  construction APIs, losing their workers.
- **tddy-acp-stub**: recorder, `hold_until_cancel`, `models`.
- **tddy-integration-tests**: new suite.

## Implementation Milestones

- [ ] `AcpDriver` spawn / initialize / open / prompt / cancel / drop
- [ ] model applied via `set_session_model`; unknown model refused naming the offered ones
- [ ] `mcp_servers` passed through
- [ ] `PermissionHandler` port; `AllowOnce`
- [ ] `claude-acp`, `codex-acp`, `opencode` moved; `claude-acp` resumes by `load_session`
- [ ] Codex auth retry as a hook
- [ ] complexity records re-measured

## Testing Plan

**Level:** integration — the driver's contract is the ACP wire to a real child process, so the tests
spawn `tddy-acp-stub` (already the pattern in `tddy-integration-tests`). The stub's recorder is what
makes client-side behaviour observable without a mock.

## Acceptance Tests

`packages/tddy-integration-tests/tests/acp_driver_acceptance.rs` (`#[serial]`, stub binary):

1. `a_prompt_returns_the_agents_text_and_stop_reason` — `open_session` + `prompt` against a two-chunk scenario → `AcpTurnEnd { stop_reason: EndTurn, text }`.
2. `a_requested_model_is_set_on_the_session` — scenario advertises `[a, b]`, open with `model: b` → recorder has `set_session_model b`.
3. `a_model_the_agent_does_not_offer_is_refused_naming_the_offered_ones` — open with `model: z` → error text contains `a` and `b`; no prompt sent.
4. `cancel_ends_a_running_turn_as_cancelled` — `hold_until_cancel` scenario, `cancel` while `prompt` runs → `stop_reason: Cancelled`, recorder has `cancel`.
5. `dropping_the_driver_terminates_the_agent_process` — stub pid from the recorder is gone after drop.
6. `mcp_servers_reach_session_new` — open with one stdio MCP server → recorder's `new_session` lists it.
7. `a_permission_request_is_answered_by_the_installed_handler` — scenario permission request, handler rejecting → the stub's recorded outcome is the rejection.
8. `a_session_id_resumes_by_session_load` — open, prompt, new driver, open with `resume: id` → recorder has `load_session id`.
9. `updates_stream_as_tool_calls_and_text` — a tool-call scenario → sink receives `ToolUse` then `ToolResult` and text in order.

`packages/tddy-integration-tests/tests/acp_backend_acceptance.rs` (extend):

10. `claude_acp_honours_the_requested_model` — `InvokeRequest.model` → recorder `set_session_model`.
11. `claude_acp_resume_survives_a_new_backend` — `SessionMode::Resume(id)` on a fresh backend → `load_session`.

`packages/tddy-integration-tests/tests/codex_acp_backend.rs` (extend):

12. `codex_acp_retries_once_through_login_on_an_auth_error` — kept passing on the driver (existing behaviour, re-pinned through the hook).

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **`set_session_model` over route args.** It is the protocol's own way and works for every agent;
  routes still override where a provider route is in force (route wins, as today).
- **The driver keeps the `?Send` ACP connection on its own thread** — the same choice the three copies
  made; only channels cross.
- **The stub records to a file** rather than exposing an RPC — the tests already pass scenarios by env
  var, and a JSONL file is inspectable after the child exits (needed by test 5).

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-acp-driver-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-agent-backend -p tddy-acp-stub -p tddy-integration-tests`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p tddy-agent-backend -p tddy-acp-stub -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
