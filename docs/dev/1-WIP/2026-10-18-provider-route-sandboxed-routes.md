# Changeset: Sandboxed CLI sessions on a provider account

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 9/9. Base: `feature/provider-route/agent-def-route`. PR: [#670](https://github.com/uppin/tddy-coder/pull/670).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-sandboxed-routes-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md` | ⚠ DURING | Unclaimed. New `spawn_opencode_pty` / `spawn_muse_pty` go in a new module; `runner.rs` gains only the dispatch arms. |
| `packages/tddy-daemon-sandbox/docs/code-issues/oversized-file-sandbox-session.md` | ⚠ DURING | New runner-env builders in a new module. |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` | ⚠ DURING | Apply the route through a helper; do not branch the function body further. |

## Responsibility

Routes inside jails. Owns the generalisation of recipe secret injection (route `secret_env` → jail
secrets; `CLAUDE_CODE_OAUTH_TOKEN` not injected when a route is present), materialising route files into
the jail scratch, sandboxed starts for `opencode-cli` and `muse-cli` (runner `--agent-kind opencode |
muse`, recipes, read-allow paths, sandbox-app `AgentKind`), and lifting node 2's "not yet supported in a
sandbox" refusal.

## Boundaries

- Does not add an egress host allowlist.
- Does not sandbox tool sessions.
- Does not change route bodies (owned by nodes 2, 3, 4, 6).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/claude-route` (2/9) | Claude route, sandboxed-start refusal | lifts the refusal; routes jailed claude-cli | change the Claude arm |
| `feature/provider-route/opencode-cli` (3/9) | descriptor, OpenCode route + config | jailed OpenCode spawn | change the descriptor type |
| `feature/provider-route/muse-cli` (6/9) | Muse descriptor + route | jailed Muse spawn | change the Muse arm |

## Draft PR contract

```rust
// tddy-sandbox-recipes
pub enum SandboxRecipe { /* … */ OpenCodeCli, MuseCli }            // policy/read arms todo!() until green
pub struct RunnerPlanRequest { /* … */ pub route: Option<HarnessRoute> }   // 3 daemon call sites pass None + TODO
pub fn route_secrets(builder: SandboxBuilder, route: &HarnessRoute) -> SandboxBuilder;
// tddy-sandbox-runner
pub mod harness_pty;
pub fn harness_pty_argv(agent_kind: &str, binary: &str, model: &str, initial_prompt: Option<&str>, resume: bool) -> Result<Vec<String>, String>;
```

`/green` also: lifts node 2's sandboxed-route refusal and passes the session's route into the three `RunnerPlanRequest` sites; teaches the sandbox app's `AgentKind` `opencode` and `muse` (no surface published — nothing above this node consumes it).

## Green wave

**Wave:** 5 of 5
**Greenable independently:** no — consumes the routes of nodes 2, 3 and 6.
**Concurrent with:** none
**Blocks:** none

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-sandbox-recipes`, `tddy-sandbox`, `tddy-sandbox-runner`, `tddy-daemon-sandbox`, `tddy-sandbox-app`,
`tddy-agent-launch` (sandboxed starts).

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-sandboxed-routes.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-sandboxed-routes.md)

## Scope

- [ ] Generalised route secrets in the recipe plan
- [ ] Route env + files into the jail map / scratch
- [ ] Jailed claude-cli with a route; refusal lifted
- [ ] Runner `--agent-kind opencode|muse`, recipes, read-allow paths, sandbox-app
- [ ] Sandboxed start paths for both TUIs

## Technical Changes

### State A
Jails `env_clear()`; only `CLAUDE_CODE_OAUTH_TOKEN` (from the daemon env) is delivered as a secret;
runner dispatches `cursor` vs claude only.

### State B
A jailed session's agent child gets the route's env and secrets; the secret never appears in the jail
env map or argv; OpenCode and Muse run jailed.

## Testing Plan

Recipe-plan tests on the built spec (env map, secret refs); runner tests on the PTY spawn command for
both new kinds; the macOS VM/cgroups suites are not part of local gates (see CI guide).

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_routed_jail_plan_carries_the_key_as_a_secret_ref_not_in_env` / `a_routed_jail_does_not_inject_the_oauth_token` | `packages/tddy-sandbox-recipes/tests/route_secrets.rs` (new) | secret refs, route env, no OAuth token beside a route |
| `the_runner_spawns_opencode_with_its_model_inside_the_jail` / `the_runner_resumes_muse_by_continuing_its_session` / `an_agent_kind_the_runner_does_not_host_is_refused` | `packages/tddy-sandbox-runner/tests/agent_kind_opencode_muse.rs` (new) | in-jail argv from the descriptors |
| `the_runner_spawns_muse_with_the_key_resolved_from_its_secret_file` | same | regression guard: a route secret resolves like the OAuth token (passes today) |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Existing secret channel**, generalised — no new delivery mechanism.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-sandbox-recipes`, `tddy-sandbox-runner` clippy `--all-targets -D warnings` clean; `tddy-daemon-sandbox` clippy `--lib` clean.

- ⚠ **Pre-existing, not this node:** `packages/tddy-daemon-sandbox/tests/sandbox_stdio_seatbelt_acceptance.rs` (`#![cfg(target_os = "macos")]`, unchanged from `master`) does not compile on macOS (`SandboxHandle` not in scope), so `--all-targets` for that crate fails locally; Linux CI never compiles it.
- `route_secrets` 2/2 fail on their assertions (the plan ignores `route`).
- `agent_kind_opencode_muse`: 3 fail at `harness_pty.rs` `todo!()`; the secret-resolution guard passes.

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-sandboxed-routes-initial-discovery.md`)
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
