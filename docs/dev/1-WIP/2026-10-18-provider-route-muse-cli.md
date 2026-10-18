# Changeset: Muse Code terminal sessions

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 6/9. Base: `feature/provider-route/default-route`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-muse-cli-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Responsibility

The `muse-cli` session type (unsandboxed). Owns the Muse descriptor, the **Muse route arm**
(`Harness::MuseCli`: `meta` preset only; key in the variable Muse Code reads for non-interactive auth),
the curated Muse model list, `MuseCliConfig { binary_path }`, and the web "Muse Code" toggle entry.

## Boundaries

- Does not add a headless `muse exec` backend (backlog entry) or sandbox Muse (node 9).
- Does not change the descriptor type (node 3 owns it).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/claude-route` (2/9) | resolver, `NotYetRoutable(MuseCli)` arm, start field, vault read | replaces the Muse arm | change precedence or other arms |
| `feature/provider-route/opencode-cli` (3/9) | `CliHarness` descriptor + registry, web toggle from a list | registers one descriptor and one toggle entry | change `CliHarness`'s fields or the claude/cursor/opencode descriptors |

## Draft PR contract

```rust
pub static MUSE_CLI: CliHarness = CliHarness { session_type: "muse-cli", route_harness: Harness::MuseCli, /* … */ };
pub const MUSE_CLI_AGENT: &str = "muse-cli";
pub fn muse_cli_models() -> Vec<BackendModel>;      // muse-spark-1.3, muse-wonder-1.0 (curated fallback)
// provider_route.rs — Muse arm: secret_env = [(MUSE_KEY_VAR, key)], env = [] ; MUSE_KEY_VAR confirmed against `muse --help`
```

## Green wave

**Wave:** 4 of 5
**Greenable independently:** no — needs node 3's descriptor (behaviour: dispatch through the registry).
**Concurrent with:** `opencode-acp` (7/9), `agent-def-route` (8/9)
**Blocks:** `sandboxed-routes` (9/9)

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-agent-launch` (descriptor), `tddy-agent-backend` (route arm, model catalog), `tddy-session-files`
(context globs), `tddy-daemon-kernel` (config), `tddy-web`.

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-muse-cli.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-muse-cli.md)

## Scope

- [ ] Wave 2 first step: record `muse --help`, `muse exec --help`, auth variable name, resume flag in discovery
- [ ] Descriptor, config, context globs (`AGENTS.md`, `.agents/skills/**`, `.muse/**`)
- [ ] Muse route arm (meta only)
- [ ] Models: account models (filter `muse-*`) or curated list
- [ ] Web toggle entry

## Technical Changes

### State A
After node 3, three descriptors exist; `resolve_route(MuseCli, _)` returns `NotYetRoutable`.

### State B
`muse-cli` starts and resumes `muse` in the worktree; on a Meta account the key is injected.

## Testing Plan

Launch acceptance with a fake `muse` binary recording argv/env; route unit tests.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_muse_session_on_a_meta_account_spawns_muse_with_the_model_and_key` | `packages/tddy-agent-launch/tests/muse_cli_session_acceptance.rs` (new) | argv/env |
| `a_muse_session_without_an_account_injects_no_key` | same | env |
| `a_resumed_muse_session_continues_its_session` | same | resume argv |
| `muse_accepts_only_meta_accounts` | `packages/tddy-agent-backend/tests/provider_route_acceptance.rs` | `PresetNotAccepted` for fireworks |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **TUI only**: `muse exec --json` / `muse serve` protocols are undocumented; a headless backend is a backlog entry.

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

- [x] Record initial discovery (`2026-10-18-provider-route-muse-cli-initial-discovery.md`)
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
