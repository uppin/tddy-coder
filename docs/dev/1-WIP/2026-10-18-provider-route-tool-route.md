# Changeset: Tool sessions with Claude and Codex agents on a provider account

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 4/9. Base: `feature/provider-route/opencode-cli`.

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
// tddy-agent-backend
impl HarnessRoute { pub fn to_file_json(&self) -> String; pub fn from_file_json(s: &str) -> Result<HarnessRoute, String>; }
pub struct InvokeRequest { /* … */ pub provider_route: Option<std::sync::Arc<HarnessRoute>> }
// provider_route.rs — Codex arm owned here
// codex: args = ["-c", "model_provider=\"tddy-route\"", "-c", "model_providers.tddy-route.base_url=\"<responses root>\"",
//                "-c", "model_providers.tddy-route.env_key=\"TDDY_ROUTE_API_KEY\"", "-c", "model_providers.tddy-route.wire_api=\"responses\""]
// tddy-coder
#[arg(long)] provider_route_path: Option<PathBuf>
pub fn load_provider_route(path: &Path) -> anyhow::Result<HarnessRoute>;   // refuses a file not mode 0600
// tddy-spawn / tool_session_spawn
// ToolSpawnPlan gains provider_route_json: Option<String>; spawner writes it like agent-def.json and adds the flag.
```

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
| `codex_on_meta_runs_exec_with_a_responses_provider_and_the_key_in_env` | `packages/tddy-agent-backend/tests/codex_provider_route.rs` (new) | argv `-c …`, env key |
| `headless_claude_on_fireworks_gets_the_anthropic_route_env` | `packages/tddy-agent-backend/tests/claude_provider_route.rs` (new) | env |
| `acp_agents_spawn_with_the_route_applied` | same files | `claude-agent-acp` / `codex-acp` child env |
| `a_route_file_is_written_owner_only_and_named_on_the_command_line` | `packages/tddy-spawn/tests/provider_route_file.rs` (new) | 0600, flag, no key on argv |
| `tddy_coder_refuses_a_route_file_others_can_read` | `packages/tddy-coder/tests/provider_route_file.rs` (new) | mode check |
| `a_cursor_tool_session_with_an_account_is_refused` | `packages/tddy-agent-launch/tests/tool_session_provider_route.rs` (new) | refusal |
| `a_resumed_tool_session_reapplies_its_route` | same | resume |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **File, not env**: keeps the daemon→coder env allowlist and the supervisor policy closed.
- **One synthetic Codex provider id** (`tddy-route`), so a person's own `~/.codex/config.toml` providers are untouched.

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

## Successor PRs

`feature/provider-route/opencode-acp` (7/9), `feature/provider-route/agent-def-route` (8/9).

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-tool-route-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
