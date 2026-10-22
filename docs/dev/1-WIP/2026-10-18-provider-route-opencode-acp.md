# Changeset: OpenCode as a tool-session agent over ACP

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (`tddy-agent-backend`, `tddy-coder`, `tddy-tools`, `tddy-integration-tests`), a spawned ACP child process, and an integration test against a scripted fake ACP agent
**Stack**: #provider-route 7/9. Base: `feature/provider-route/muse-cli`. PR: [#668](https://github.com/uppin/tddy-coder/pull/668).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-opencode-acp-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ DURING | Unclaimed. Add only the new variant's arms and list entries; do not split the file here (seams B/C are its own restructure). |
| `packages/tddy-coder/docs/code-issues/oversized-file-run.md` | ⚠ DURING | One `create_backend` arm, one id, one path flag. |

## Responsibility

`AnyBackend::OpenCode` over `opencode acp`, and its registration in every backend list. Owns `OpenCodeAcpBackend` (`backend/opencode_acp.rs`), its `with_provider_route` builder (in `backend/routed.rs`), `BackendCliPaths.opencode_cli_path`, `tddy-tools list-models --opencode-cli-path`, `--opencode-cli-path` / `TDDY_OPENCODE_CLI` / coder-config `opencode_cli_path` in `tddy-coder`, the `opencode` entries of `backend_selection_question`, `backend_from_label`, `default_model_for_agent`, `curated_models_for_agent`, `preselected_index_for_agent`, `model_catalog::build_backend`, `BUILTIN_BACKEND_AGENT_IDS`, and the `opencode` rows in `allowed_agents` of the dev and desktop configs. **Not** the `opencode` row of `context_globs_for_agent` — node 3 (`opencode-cli`) owns it and this node consumes it.

## Boundaries

- Does not change other backends or the order of existing selection entries (append only).
- Does not change `opencode_config_for` (node 3) or the route file (node 4).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/opencode-cli` (3/9) | `opencode_config_for(target)`, OpenCode route arm (`Harness::OpenCode`) | the ACP child gets `OPENCODE_CONFIG` from that file | generate config differently |
| `feature/provider-route/tool-route` (4/9) | route file → `InvokeRequest.provider_route` | applies it on the ACP child | change the file format or loader |

## Draft PR contract

```rust
// tddy-agent-backend/src/backend/opencode_acp.rs
pub struct OpenCodeAcpBackend { agent_path: PathBuf }
impl OpenCodeAcpBackend { pub const DEFAULT_CLI_BINARY: &str = "opencode"; pub fn new() -> Self; pub fn with_agent_path(path: PathBuf) -> Self; }
impl CodingBackend for OpenCodeAcpBackend { fn name(&self) -> &str { "opencode" } /* invoke, list_models: todo!() */ }
pub enum AnyBackend { /* … */ OpenCode(OpenCodeAcpBackend) }   // five dispatch arms delegate
// backend/routed.rs
impl OpenCodeAcpBackend { pub fn with_provider_route(self, route: HarnessRoute) -> Self; }
// model_catalog.rs
pub struct BackendCliPaths { /* … */ pub opencode_cli_path: Option<PathBuf> }
// tddy-tools list-models
--opencode-cli-path <PATH>
```

The existing pinned list tests in `backend/mod.rs` (`backend_selection_question_returns_six_options…`, label order, preselected indexes) are updated by `/green` together with the registration: a shared list, appended to.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** no — routed test applies node 4's route plumbing and node 3's config.
**Concurrent with:** `muse-cli` (6/9), `agent-def-route` (8/9)
**Blocks:** none

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-agent-backend`, `tddy-coder`, `tddy-tools`, `tddy-daemon-rpc` (list-models probe args), configs
(`dev.daemon.yaml`, `dev.desktop.yaml`, `desktop.yaml.production`), `tddy-integration-tests`.

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-opencode-acp.md](../../ft/coder/1-WIP/PRD-2026-10-18-provider-route-opencode-acp.md)

## Scope

- [ ] Backend over ACP (reuse the ACP worker shape of `ClaudeAcpBackend`; no new protocol code)
- [ ] All list sites (see discovery Exploration 2 §4.4 items 1–11); pinned-list tests updated
- [ ] Path flag/env/config; list-models
- [ ] Route application on the ACP child
- [ ] `allowed_agents` rows

## Technical Changes

### State A
Six backends; tests pin `backend_selection_question` at six options.

### State B
Seven backends; `opencode` selectable and listable; routed through node 3's config.

## Testing Plan

Integration test against a scripted fake ACP agent (same harness as `codex_acp_backend.rs`), list-model
test, updated pinned-list tests.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `tddy_coder_completes_a_turn_with_opencode_over_acp` / `opencode_lists_the_models_its_agent_advertises` / `list_models_asks_the_opencode_binary_it_is_given` / `a_routed_opencode_agent_gets_the_generated_config_and_key` | `packages/tddy-integration-tests/tests/opencode_acp_backend.rs` (new) | ACP turn against `tddy-acp-stub`, model listing, routed child env |
| `backend_selection_question_appends_opencode` / `opencode_has_a_default_model_in_its_curated_list` | `packages/tddy-agent-backend/tests/opencode_registration.rs` (new) | appended entry; a `provider/model` default, not the Claude fallback |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **ACP only** for the headless backend; OpenCode's TUI is node 3.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-agent-backend`, `tddy-tools`, `tddy-integration-tests` (`opencode_acp_backend` target) — clippy `-D warnings` clean.

- `opencode_acp_backend` 4/4 fail at this node's `todo!()` (`opencode_acp.rs`, `routed.rs`) or its missing `build_backend` arm (`unknown agent`).
- `opencode_registration` 2/2 fail (no appended entry; default falls back to Claude's — the first draft of that test passed vacuously and was tightened).

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-opencode-acp-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (per-node gate waived by the developer on 2026-10-18 — "finish remaining waves without stopping"; review at stack hand-off)
- [x] TDD Red — write failing unit/integration tests
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
