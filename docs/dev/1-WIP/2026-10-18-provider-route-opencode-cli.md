# Changeset: OpenCode terminal sessions and the CLI harness descriptor

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 3/9. Base: `feature/provider-route/claude-route`. PR: [#664](https://github.com/uppin/tddy-coder/pull/664).

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
registers through. Owns `tddy_agent_backend::cli_harness::{CliHarness, CliLaunch, CliModelSource,
cli_harnesses, cli_harness, models_from_listing}` (in the lowest crate every session-type consumer
already reaches through `tddy_core`), the claude-cli and cursor-cli descriptors (behaviour-preserving —
`CliSessionManager::build_{claude,cursor}_argv` delegate to them), the OpenCode descriptor, the
**OpenCode route arm** of `resolve_route` (`Harness::OpenCodeCli`, `Harness::OpenCode`) with
`opencode_config_for`, `OPENCODE_ROUTE_PROVIDER_ID`, `OPENCODE_ROUTE_KEY_ENV`, the `opencode` row of
`context_globs_for_agent` (node 7 consumes it), `tddy_agent_launch::cli_route_launch`, the daemon
config map `cli_harnesses.<session type>.binary_path` with `DaemonConfig::cli_harness_binary`, and the
web `"opencode-cli"` session type and toggle entry.

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
// tddy-agent-backend/src/cli_harness.rs
pub struct CliLaunch<'a> { binary, model, session_id, conversation_id: Option<&str>, initial_prompt: Option<&str>,
                           permission_mode: Option<&str>, dangerously_skip_permissions: bool, resume: bool }
pub enum CliModelSource { Curated(fn() -> BackendModels), ListedByBinary(&'static [&'static str]) }
pub struct CliHarness { session_type, label, route_harness: Harness, context_agent, default_binary, binary_env,
                        route_config_env: Option<&'static str>, argv: fn(&CliLaunch) -> Vec<String>, models: CliModelSource }
pub fn cli_harnesses() -> &'static [CliHarness];          // claude-cli, cursor-cli, opencode-cli
pub fn cli_harness(session_type: &str) -> Option<&'static CliHarness>;
pub fn models_from_listing(stdout: &str) -> BackendModels; // `opencode models`
// tddy-agent-backend/src/provider_route.rs (OpenCode arm and helpers owned here)
pub const OPENCODE_ROUTE_PROVIDER_ID: &str = "tddy-route";
pub const OPENCODE_ROUTE_KEY_ENV: &str = "TDDY_OPENCODE_PROVIDER_KEY";
pub fn opencode_config_for(target: &RouteTarget) -> RouteFile;   // opencode.json, key via {env:…}
// route for OpenCode: files = [opencode.json], secret_env = [(OPENCODE_ROUTE_KEY_ENV, key)], args = ["--model", "tddy-route/<model>"]
// tddy-agent-launch/src/cli_route_launch.rs
pub struct CliRouteLaunch { pub env: Vec<(String, String)>, pub args: Vec<String> }
pub fn cli_route_launch(harness: &CliHarness, route: Option<&HarnessRoute>, session_dir: &Path) -> io::Result<CliRouteLaunch>;
// tddy-daemon-kernel config
pub cli_harnesses: BTreeMap<String, CliHarnessConfig>;   // `cli_harnesses: { opencode-cli: { binary_path } }`
impl DaemonConfig { pub fn cli_harness_binary(&self, session_type: &str, default_binary: &str, binary_env: &str) -> String; }
```

OpenCode argv (from https://opencode.ai/docs/cli/): start `opencode [--model provider/model] [--prompt <p>]`,
resume `opencode --continue` (one session per worktree, so the last session there is this one).
Web: `SessionType` gains `"opencode-cli"`; `create-session-type-opencode-cli` toggle.

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
| `the_registry_offers_claude_cursor_and_opencode_in_order` / `a_tool_or_workspace_session_is_no_cli_harness` / `opencode_is_routed_as_opencode_and_reads_its_config_from_opencode_config` | `packages/tddy-agent-backend/tests/cli_harness_acceptance.rs` (new) | registry |
| `an_opencode_session_starts_with_its_model_and_first_prompt` / `a_resumed_opencode_session_continues_the_worktrees_last_session` / `opencode_models_are_read_one_provider_model_per_line` | same | argv, `--continue`, model listing |
| `claude_cli_and_cursor_cli_argv_are_unchanged_through_the_descriptor` | `packages/tddy-cli-sessions/tests/cli_harness_parity.rs` (new) | parity with today's builders |
| `opencode_on_meta_prefers_responses_and_carries_reasoning` / `opencode_on_baseten_uses_chat_completions` / `the_generated_config_holds_no_key` | `packages/tddy-agent-backend/tests/opencode_route_acceptance.rs` (new) | generated `opencode.json` |
| `an_opencode_route_carries_the_config_the_key_and_the_model_argument` / `opencode_refuses_an_account_that_serves_neither_openai_protocol` | same | the OpenCode arm of `resolve_route` |
| `an_opencode_session_on_fireworks_gets_a_config_naming_that_endpoint` / `an_unrouted_launch_adds_nothing_and_writes_nothing` | `packages/tddy-agent-launch/src/cli_route_launch_tests.rs` (new) | 0600 file, `OPENCODE_CONFIG`, key env, args |
| `an_opencode_session_is_served_opencodes_context_files` / `claude_cli_and_cursor_cli_keep_their_context_agents` | `packages/tddy-session-files/tests/opencode_context_files.rs` (new) | context agent + globs (second test is a regression guard and passes today) |
| Cypress: offers OpenCode / lists the daemon's OpenCode models / starts an opencode-cli session | `packages/tddy-web/cypress/component/CreateSessionOpenCodeAcceptance.cy.tsx` (new) | toggle and request |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Descriptor in `tddy-agent-backend`** (wave 2): every crate that matches session types reaches it
  through `tddy_core`; putting it in `tddy-agent-launch` would leave `tddy-cli-sessions`,
  `tddy-session-files` and the runner unable to use it.
- **One config map** `cli_harnesses:` (wave 2) instead of an `opencode_cli:` section, so node 6 adds no
  config field.
- **Resume = `--continue`**: tddy never learns OpenCode's own session id; one session per worktree
  makes "the last session here" exact.
- **Descriptor now, not later**: two new CLI types in this stack; the descriptor pays for itself at the second.
- **Key via `{env:…}` in config**, not written into `opencode.json`, so the file holds no secret.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-agent-backend`, `tddy-agent-launch`, `tddy-cli-sessions`, `tddy-session-files`,
`tddy-daemon-kernel` (clippy `--all-targets -D warnings` clean); one Cypress spec.

- `cli_harness_acceptance` 6/6, `cli_harness_parity` 1/1, `cli_route_launch_tests` 2/2 fail at
  `cli_harness.rs` `todo!()` (this node).
- `opencode_route_acceptance`: 3 fail at `opencode_config_for` (this node); 2 fail first at node 2's
  `resolve_route` `todo!()` — the OpenCode arm lives inside it. Sequencing: green after #663.
- `opencode_context_files`: 1 fails (no `opencode` row yet); 1 regression guard passes.
- `CreateSessionOpenCodeAcceptance.cy.tsx` 3/3 fail: no OpenCode toggle.
- OpenCode is not installed on this machine; its CLI/config contract is taken from opencode.ai docs
  (recorded in discovery). `/green` must confirm against a real `opencode --help`.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
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
