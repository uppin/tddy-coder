# Changeset: OpenCode as a tool-session agent over ACP

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 7/9. Base: `feature/provider-route/muse-cli`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-opencode-acp-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ DURING | Unclaimed. Add only the new variant's arms and list entries; do not split the file here (seams B/C are its own restructure). |
| `packages/tddy-coder/docs/code-issues/oversized-file-run.md` | ⚠ DURING | One `create_backend` arm, one id, one path flag. |

## Responsibility

`AnyBackend::OpenCode` over `opencode acp`, and its registration in every backend list. Owns
`OpenCodeAcpBackend`, `--opencode-cli-path` / `TDDY_OPENCODE_CLI` / coder-config `opencode_cli_path`,
`BackendCliPaths.opencode_cli_path`, `tddy-tools list-models --opencode-cli-path`, and the `opencode`
rows in `allowed_agents` of the dev and desktop configs.

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
pub struct OpenCodeAcpBackend { /* agent path */ }
impl OpenCodeAcpBackend { pub fn new() -> Self; pub fn with_agent_path(path: PathBuf) -> Self; }
impl CodingBackend for OpenCodeAcpBackend { fn name(&self) -> &str { "opencode" } /* … */ }
pub enum AnyBackend { /* … */ OpenCode(OpenCodeAcpBackend) }
```

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
| `tddy_coder_completes_a_turn_with_opencode_over_acp` | `packages/tddy-integration-tests/tests/opencode_acp_backend.rs` (new) | ACP turn |
| `opencode_lists_the_models_its_agent_advertises` | same | `list_models` |
| `a_routed_opencode_agent_gets_the_generated_config_and_key` | same | child env |
| `backend_selection_question_appends_opencode` | `packages/tddy-agent-backend/src/backend/mod.rs` (tests) | seven options, existing order |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **ACP only** for the headless backend; OpenCode's TUI is node 3.

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

- [x] Record initial discovery (`2026-10-18-provider-route-opencode-acp-initial-discovery.md`)
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
