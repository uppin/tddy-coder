# Changeset: OpenCode terminal sessions and the CLI harness descriptor

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 3/9. Base: `feature/provider-route/claude-route`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-opencode-cli-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines.md](../todo/2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines.md) | ⚠ DURING | Expressing cursor-cli through the descriptor must not grow `cursor_cli_spawn.rs`; new code goes in new modules. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ DURING | The dispatch on `session_type` becomes a descriptor lookup — expected to reduce the function's branches, not add to them. |

## Responsibility

The `opencode-cli` session type end-to-end (unsandboxed), and the **CLI harness descriptor** it
registers through. Owns `tddy_agent_launch::cli_harness::{CliHarness, CliHarnessRegistry, cli_harness}`
(or the crate wave 2 finds the session-type matches converge in), the claude-cli and cursor-cli
descriptors (behaviour-preserving), the OpenCode descriptor, the **OpenCode route arm** of
`resolve_route` (`Harness::OpenCodeCli` and `Harness::OpenCode` — the config generator
`opencode_config_for(target) -> RouteFile`), `OpenCodeCliConfig { binary_path }` in daemon config, and
the web "OpenCode" toggle.

## Boundaries

- Does not sandbox OpenCode (node 9) or add the ACP backend (node 7).
- Does not change claude-cli / cursor-cli argv or env — the descriptor must reproduce them exactly.
- Does not touch the route matrix rows other than OpenCode's.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/accounts` (1/9) | provider accounts, `ListProviderAccountModels` | model list for an account-backed OpenCode session | change accounts |
| `feature/provider-route/claude-route` (2/9) | `resolve_route`, `HarnessRoute`, `RouteTarget`, `provider_account_id` on start/metadata, `SessionAccountAccess::provider_account`, `CreateSessionProviderField`, `ListProviderRoutes` | replaces the `NotYetRoutable(OpenCode*)` arm; applies routes through the descriptor | add fields to `StartSessionRequest`, change precedence, or add a second matrix |

## Draft PR contract

```rust
pub struct CliHarness {
    pub session_type: &'static str,        // "claude-cli" | "cursor-cli" | "opencode-cli"
    pub route_harness: Harness,
    pub context_agent: &'static str,
    pub binary: fn(&DaemonConfig) -> PathBuf,
    pub argv: fn(&CliLaunch) -> Vec<String>,   // model, resume id, initial prompt, extra args
    pub models: CliModelSource,               // Curated(fn) | Probe(fn) | Account
}
pub fn cli_harness(session_type: &str) -> Option<&'static CliHarness>;
pub fn cli_session_types() -> impl Iterator<Item = &'static str>;
// provider_route.rs (arm owned here)
pub fn opencode_config_for(target: &RouteTarget) -> RouteFile;   // opencode.json; key via {env:TDDY_OPENCODE_PROVIDER_KEY}
```

Web: `SessionType` gains `"opencode-cli"`; `CreateSessionTypeToggle` renders from a list.

## Green wave

**Wave:** 3 of 5
**Greenable independently:** no — its routed-start test applies node 2's resolver output through node 2's start path.
**Concurrent with:** `tool-route` (4/9), `default-route` (5/9)
**Blocks:** `muse-cli` (6/9, registers a descriptor), `opencode-acp` (7/9, reuses `opencode_config_for`), `sandboxed-routes` (9/9)

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-agent-launch`, `tddy-agent-backend` (route arm, model catalog pseudo-agent), `tddy-cli-sessions`
(argv), `tddy-changeset`, `tddy-session-files` (context globs), `tddy-session-activity`,
`tddy-terminal-rpc` / `tddy-tools` (pty relay), `tddy-daemon-kernel` (config), `tddy-web`.

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-opencode-cli.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-opencode-cli.md)

## Summary

A third CLI session type, added through a descriptor rather than a thirtieth string match.

## Scope

- [ ] Descriptor + registry; claude-cli and cursor-cli expressed through it (no behaviour change)
- [ ] OpenCode descriptor: binary, argv (`opencode [--model provider/model] [--session id] [--prompt …]` — confirm flags against `opencode --help` in wave 2), context globs
- [ ] OpenCode route arm + `opencode.json` generator (Responses via `@ai-sdk/openai` with `reasoningEffort`, `include: ["reasoning.encrypted_content"]`; Chat via `@ai-sdk/openai-compatible`)
- [ ] Start, resume, metadata, listing, pty relay for `opencode-cli`
- [ ] Model list: account models or `opencode models`
- [ ] Config `opencode_cli.binary_path`; web toggle

## Technical Changes

### State A
~30 files match `"claude-cli"` / `"cursor-cli"` (start dispatch, resume, metadata, listing, telegram
pickers, pty relay, placement, context files, sandbox runner, web toggle). No OpenCode anywhere.

### State B
Session-type dispatch, resume, model validation and context-agent mapping read the descriptor; three
descriptors registered. An OpenCode session on an account receives `OPENCODE_CONFIG=<session>/opencode.json`
and the key in `TDDY_OPENCODE_PROVIDER_KEY`.

### Delta
As Scope. Telegram pickers and placement keep their claude-only rules (split / sandboxed codebase stay claude-cli only).

## Implementation Milestones
- [ ] M1 Descriptor + claude/cursor through it, existing suites green
- [ ] M2 OpenCode descriptor + route arm
- [ ] M3 Start/resume/listing + web

## Testing Plan

Descriptor parity tests (argv/env for claude-cli and cursor-cli equal to today's builders), OpenCode
argv and config unit tests, launch-level acceptance with a fake `opencode` binary, Cypress toggle test.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `claude_cli_and_cursor_cli_argv_are_unchanged_through_the_descriptor` | `packages/tddy-agent-launch/tests/cli_harness_descriptor_acceptance.rs` (new) | parity with today's builders |
| `an_opencode_session_spawns_opencode_in_the_worktree_with_its_model` | same | argv, cwd |
| `an_opencode_session_on_fireworks_gets_a_config_naming_that_endpoint` | same | `OPENCODE_CONFIG` file 0600, key only in env |
| `a_resumed_opencode_session_reopens_its_session_id` | same | resume argv |
| `opencode_on_meta_prefers_responses_and_carries_reasoning` | `packages/tddy-agent-backend/tests/provider_route_acceptance.rs` | generated JSON |
| `opencode_on_baseten_uses_chat_completions` | same | adapter choice |
| `the_pane_offers_opencode_and_lists_its_models` | `packages/tddy-web/cypress/component/CreateSessionOpenCodeAcceptance.cy.tsx` (new) | toggle + models |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Descriptor now, not later**: two new CLI types in this stack; the descriptor pays for itself at the second.
- **Key via `{env:…}` in config**, not written into `opencode.json`, so the file holds no secret.

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

`feature/provider-route/muse-cli` (6/9), `feature/provider-route/opencode-acp` (7/9), `feature/provider-route/sandboxed-routes` (9/9).

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-opencode-cli-initial-discovery.md`)
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
