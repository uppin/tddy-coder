# Changeset: Tool sessions with Claude and Codex agents on a provider account

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (tddy-agent-backend, tddy-agent-launch, tddy-spawn, tddy-coder, tddy-web), a spawned-process seam (stub binaries record argv/env), and an owner-only file contract chowned to the session user
**Stack**: #provider-route 4/9. Base: `feature/provider-route/opencode-cli`. PR: [#665](https://github.com/uppin/tddy-coder/pull/665).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-tool-route-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-coder/docs/code-issues/oversized-file-run.md` | ⚠ DURING | Unclaimed. The flag and the loader go in a new module (`provider_route_file.rs`); `run.rs` gains only the flag field and one call. |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ DURING | `InvokeRequest` gains one field; route application lives in each backend's file. |

## Responsibility

Delivering a route to a **tool session** and applying it in the headless backends. Owns the route file
format and writer (`<session_dir>/provider-route.json`, 0600, chowned), `tddy-coder
--provider-route-path`, `InvokeRequest.provider_route: Option<Arc<HarnessRoute>>` (or the backend
constructor equivalent wave 2 settles on), route application in `ClaudeCodeBackend`, `ClaudeAcpBackend`,
`CodexBackend` (`-c model_provider…` argv) and `CodexAcpBackend`, the **Codex route arm** of
`resolve_route` (`Harness::Codex`, `Harness::CodexAcp`), and enabling the web picker for tool sessions.

## Boundaries

- Does not add OpenCode as a backend (node 7) or apply routes to `cursor` / `stub` / specialized defs (refused).
- Does not widen `SESSION_GIT_ENVIRONMENT_KEYS` or the supervisor's `allowed_env_keys`.
- Does not touch PTY sessions.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/claude-route` (2/9) | `resolve_route` + Claude arms, `HarnessRoute`, `provider_account_id` start field + metadata, vault read, web picker | resolves the route for tool-session agents; serialises it to the file; enables the picker | change the matrix rows of Claude, precedence, or the proto |
| `feature/provider-route/opencode-cli` (3/9) | descriptor-based session dispatch | nothing beyond the line position — tool sessions are not CLI sessions | — |

## Draft PR contract

```rust
// tddy-agent-backend/src/provider_route.rs — Codex arm owned here
pub const CODEX_ROUTE_PROVIDER_ID: &str = "tddy-route";
pub const CODEX_ROUTE_KEY_ENV: &str = "TDDY_ROUTE_API_KEY";
// route for codex / codex-acp: args = -c model_provider="tddy-route", -c model_providers.tddy-route.{name,base_url,env_key,wire_api="responses"}, -m <model>; secret_env = [(TDDY_ROUTE_API_KEY, key)]
// tddy-agent-backend/src/provider_route_file.rs
pub const PROVIDER_ROUTE_FILENAME: &str = "provider-route.json";
pub fn route_to_json(route: &HarnessRoute) -> String;
pub fn route_from_json(json: &str) -> Result<HarnessRoute, String>;
pub fn load_route_file(path: &Path) -> Result<HarnessRoute, String>;   // refuses group/other-readable
// tddy-agent-backend/src/backend/routed.rs (all builders in one module; the backend files are oversized)
impl AnyBackend { pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, String>; } // cursor, stub refused
impl ClaudeCodeBackend | ClaudeAcpBackend | CodexBackend | CodexAcpBackend { pub fn with_provider_route(self, route: HarnessRoute) -> Self; }
// tddy-spawn
pub fn write_provider_route_file(session_dir: &Path, route_json: &str, os_user: &str) -> anyhow::Result<PathBuf>;
SpawnOptions.provider_route_json: Option<&str>          // never argv or env; child gets --provider-route-path
// tddy-agent-launch
ToolSpawnPlan.provider_route_json: Option<String>;
pub fn tool_session_route_json(access: &SessionAccountAccess, agent: &str, sources: &RouteSources, model: &str) -> Result<Option<String>, Status>;
// tddy-coder
Args/CoderArgs.provider_route_path: Option<PathBuf>     // --provider-route-path
```

The route is applied through a per-backend builder, not an `InvokeRequest` field: `InvokeRequest` has 32 struct literals across the workspace. The ACP builders hand the route to their worker, which applies it when it spawns the agent.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** no — needs node 2's resolver and start field (behaviour); concurrent with nodes 3 and 5.
**Concurrent with:** `opencode-cli` (3/9), `default-route` (5/9)
**Blocks:** `opencode-acp` (7/9, uses the route file), `agent-def-route` (8/9, profiles start tool sessions)

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-agent-backend`, `tddy-coder`, `tddy-spawn`, `tddy-agent-launch` (tool session start/resume),
`tddy-web` (picker for tool sessions).

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-tool-route.md](../../ft/coder/1-WIP/PRD-2026-10-18-provider-route-tool-route.md)

## Summary

Routes reach headless agents through a 0600 file, the same way agent definitions carry credentials today.

## Scope

- [ ] Route file format, writer (daemon), loader (`tddy-coder`, mode check)
- [ ] `InvokeRequest.provider_route` plumbed from `create_backend`
- [ ] Claude headless + ACP: route env and secret env on the child
- [ ] Codex arm + `codex exec` argv + key env; `codex-acp` overrides (confirm whether `codex-acp` accepts `-c`; else a per-session `CODEX_HOME` overlay)
- [ ] Refusal for unsupported agents
- [ ] Web picker for tool sessions filtered by `ListProviderRoutes(agent)`

## Technical Changes

### State A
`spawner.rs` env allowlist `SESSION_GIT_ENVIRONMENT_KEYS` (5 keys, "Never a credential"); agent defs
travel as `<session_dir>/agent-def.json` 0600 + `--agent-def-path`. Backends inherit `tddy-coder`'s
env and set only `PATH`/`TDDY_*`. `build_codex_exec_argv` has no `-c`.

### State B
A routed tool session's backend child has the route's env and secret env; Codex gets the provider
overrides; nothing secret is on argv or in the daemon→coder env.

### Delta
As Scope.

## Implementation Milestones
- [ ] M1 File format + loader + plumbing
- [ ] M2 Claude backends
- [ ] M3 Codex backends + arm
- [ ] M4 Web

## Testing Plan

Backend unit tests on the built `Command` (args/env) with a route; `tddy-coder` integration test that
loads a route file and drives the stub-recorded command; spawner test for file mode and flag.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `codex_on_meta_declares_a_responses_provider_through_config_overrides` / `codex_refuses_an_account_without_responses` | `packages/tddy-agent-backend/tests/codex_route_acceptance.rs` (new) | Codex arm |
| `a_route_round_trips_through_its_file_json` / `an_owner_only_route_file_loads` / `tddy_coder_refuses_a_route_file_others_can_read` | `packages/tddy-agent-backend/tests/provider_route_file.rs` (new) | route file |
| `codex_on_meta_runs_exec_with_a_responses_provider_and_the_key_in_env` / `headless_claude_on_fireworks_gets_the_anthropic_route_env` / `acp_agents_spawn_with_the_route_applied` / `cursor_and_the_stub_cannot_be_routed` | `packages/tddy-integration-tests/tests/tool_session_provider_route.rs` (new) | stub binaries record argv/env |
| `a_route_file_is_written_owner_only_in_the_session_directory` / `rewriting_a_route_file_resecures_it` | `packages/tddy-spawn/tests/provider_route_file.rs` (new) | writer |
| `a_codex_tool_session_on_meta_is_handed_a_route_file_with_its_provider` / `a_cursor_tool_session_with_an_account_is_refused` / `a_specialized_agent_definition_with_an_account_is_refused` / `a_tool_session_that_chooses_nothing_gets_no_route_file` | `packages/tddy-agent-launch/src/tool_session_route_tests.rs` (new) | real vault |
| Cypress: asks routes for the selected agent / sends the account with a codex session / cursor shows why | `packages/tddy-web/cypress/component/CreateSessionToolProviderAcceptance.cy.tsx` (new) | picker for tool sessions |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Builder, not an `InvokeRequest` field** (wave 2): 32 literals of a non-`Default`-constructed struct; a builder touches the four backends only.
- **Telegram-started tool sessions pass no route yet** (`TODO(tool-route)` at its `SpawnOptions`): the Telegram start has no account picker; out of scope.
- **File, not env**: keeps the daemon→coder env allowlist and the supervisor policy closed.
- **One synthetic Codex provider id** (`tddy-route`), so a person's own `~/.codex/config.toml` providers are untouched.
- **Codex display names are a closed table** (`CODEX_ROUTE_PRESET_NAMES`): `meta` → "Meta" and `openai-compatible` → "OpenAI-compatible", the only presets that serve Responses. A preset outside the table is refused, not named by a guess. The labels duplicate `tddy_accounts::presets()`, and no test links the two.
- **codex-acp takes `-c` overrides** (checked with `codex-acp --help`), so its route is the same overrides with `-m` rewritten to `-c model=`. Claude ACP takes its model from its environment, so its route arguments are not passed.
- **ACP workers start on first use**, not at construction, so a route set after construction reaches the agent child. The spawn error surfaces through the command channel.
- **Route file mode is owner-only** (no group or other bit), stricter than "not readable by others": a group-writable file could swap the endpoint and key.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-agent-backend`, `tddy-agent-launch`, `tddy-spawn`, `tddy-coder`, `tddy-telegram-control`, `tddy-integration-tests` (`tool_session_provider_route` target) — clippy `-D warnings` clean.

- `provider_route_file` 3/3, `tool_session_provider_route` 4/4 (`routed.rs`), `tddy-spawn provider_route_file` 2/2 fail at this node's `todo!()`.
- `codex_route_acceptance` 2/2 fail first at node 2's `resolve_route` `todo!()` (the Codex arm lives inside it) — green after #663.
- `tool_session_route_tests` 4/4 fail first at node 1's `preset()` — green after #662.
- `CreateSessionToolProviderAcceptance.cy.tsx` 3/3 fail: the picker is node 2's to render — green after #663 plus this node's tool-session wiring.

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## Successor PRs

`feature/provider-route/opencode-acp` (7/9), `feature/provider-route/agent-def-route` (8/9).

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-tool-route-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (per-node gate waived by the developer on 2026-10-18 — "finish remaining waves without stopping"; review at stack hand-off)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
