# Changeset: Muse Code terminal sessions

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 6/9. Base: `feature/provider-route/default-route`. PR: [#667](https://github.com/uppin/tddy-coder/pull/667).

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
// tddy-agent-backend/src/cli_harness_muse.rs
pub const MUSE_ROUTE_KEY_ENV: &str = "META_API_KEY";   // secondary sources; /green confirms against `muse --help`
pub fn muse_cli_models() -> BackendModels;              // muse-spark-1.3 (default), muse-wonder-1.0
fn muse_argv(launch: &CliLaunch) -> Vec<String>;        // start: muse [--model m] [prompt]; resume: muse --continue
pub static MUSE_CLI: CliHarness = CliHarness { session_type: "muse-cli", label: "Muse Code", route_harness: Harness::MuseCli,
    context_agent: "muse", default_binary: "muse", binary_env: "TDDY_MUSE_CLI", route_config_env: None, argv: muse_argv,
    models: CliModelSource::Curated(muse_cli_models) };
// Muse arm of resolve_route (owned here): meta only; secret_env = [(MUSE_ROUTE_KEY_ENV, key)]; no env, files or args
// green also: register MUSE_CLI in cli_harnesses(); the `muse` row of context_globs_for_agent (AGENTS.md, .agents/skills/**, .muse/**)
```

Web: `SessionType` gains `"muse-cli"`; toggle `create-session-type-muse-cli`.

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
| `muse_code_is_registered_as_a_cli_harness` / `a_muse_session_starts_with_its_model_and_first_prompt` / `a_resumed_muse_session_continues_its_session` / `muse_on_meta_is_handed_the_key_and_nothing_else` / `muse_accepts_only_meta_accounts` / `the_curated_muse_models_default_to_spark` | `packages/tddy-agent-backend/tests/muse_cli_acceptance.rs` (new) | descriptor, argv, route arm, models |
| Cypress: starts a muse-cli session with a Muse model | `packages/tddy-web/cypress/component/CreateSessionMuseAcceptance.cy.tsx` (new) | toggle + request |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Registry tripwire**: node 3's `the_registry_offers_claude_cursor_and_opencode_in_order` lists the registry exactly; registering `muse-cli` here must extend that expected list in the same commit (a shared registry, not node 3's behaviour changing).
- **Muse CLI facts are unconfirmed** (`META_API_KEY`, `--model`, `--continue`): Muse Code is not installed here. `/green` begins by recording `muse --help` in discovery and adjusting these tests to what it says.
- **TUI only**: `muse exec --json` / `muse serve` protocols are undocumented; a headless backend is a backlog entry.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-agent-backend` clippy `-D warnings` clean.

- `muse_cli_acceptance`: 1 fails at this node's `muse_cli_models` `todo!()`; 3 first at node 3's `cli_harness` `todo!()`; 2 first at node 2's `resolve_route` `todo!()` — green after #663/#664.
- `CreateSessionMuseAcceptance.cy.tsx` 1/1 fails: no Muse toggle.

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
