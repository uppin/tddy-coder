# Changeset: Claude CLI sessions on a provider account — the route resolver

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 2/9. Base: `feature/provider-route/accounts`.

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-claude-route-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ DURING | Unclaimed. Resolve the route in a helper called from `start_session_core`; do not add branches to its body. |
| `packages/tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | ⚠ DURING | Unclaimed. The resolver is a new module `backend/provider_route.rs` (or `src/provider_route.rs`); `mod.rs` gains only the `mod`/`pub use` lines. |

## Responsibility

The **route**: owns `tddy_agent_backend::provider_route::{Harness, RouteTarget, HarnessRoute,
RouteRefusal, RouteSources, RouteChoice, resolve_route, choose_route}` and the capability matrix for
**every** harness (claude, claude-acp, claude-cli, codex, codex-acp, opencode, opencode-cli, muse-cli,
cursor, cursor-cli) — but **implements route bodies for the Claude harnesses only**; other harnesses'
arms return `RouteRefusal::NotYetRoutable(harness)` until their node replaces that arm. Owns
`StartSessionRequest.provider_account_id`, `SessionMetadata.provider_account_id`,
`catalog.ListProviderRoutes`, the claude-cli start/resume application, the vault read
`SessionAccountAccess::provider_account`, and the web `CreateSessionProviderField`.

## Boundaries

- Does not apply routes to tool sessions (node 4), OpenCode (node 3/7), Muse (node 6), or jails (node 9).
- Does not feed `RouteSources.user_default` (node 5) or `.agent_definition` (node 8).
- Does not change account storage, presets or the probe (node 1).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/accounts` (1/9) | `ProviderAccount::from_record`, `ProviderEndpoints`, `WireProtocol`, `ListProviderAccountModels` RPC, preset ids | reads the chosen account from the owner's vault at start; the web picker lists models through the RPC | change presets, the record codec, the probe, or `accounts.proto` |

## Draft PR contract

```rust
// tddy-agent-backend/src/provider_route.rs
pub enum Harness { Claude, ClaudeAcp, ClaudeCli, Codex, CodexAcp, OpenCode, OpenCodeCli, MuseCli, Cursor, CursorCli }
impl Harness { pub fn from_agent_or_session_type(id: &str) -> Option<Harness>; }
pub struct RouteTarget { pub preset: String, pub endpoints: RouteEndpoints, pub api_key: RouteSecret, pub model: String }
pub struct RouteEndpoints { pub anthropic_messages: Option<String>, pub openai_responses: Option<String>, pub openai_chat: Option<String> }
pub struct RouteSecret(/* redacted Debug */ String);
pub struct RouteFile { pub name: String, pub contents: String }      // written 0600 in the session dir
pub struct HarnessRoute { pub env: Vec<(String, String)>, pub secret_env: Vec<(String, RouteSecret)>, pub files: Vec<RouteFile>, pub args: Vec<String> }
pub enum RouteRefusal { HarnessCannotBeRouted { harness: Harness }, ProtocolNotServed { harness: Harness, needs: &'static str }, PresetNotAccepted { harness: Harness, preset: String }, NotYetRoutable(Harness) }
pub fn resolve_route(harness: Harness, target: &RouteTarget) -> Result<HarnessRoute, RouteRefusal>;
pub fn route_supported(harness: Harness, preset: &str, protocols: &[&str]) -> Result<(), RouteRefusal>;

pub enum RouteChoice { BuiltIn, Account(String) }        // explicit "Anthropic (built in)" vs an account id
pub struct RouteSources { pub session: Option<RouteChoice>, pub agent_definition: Option<RouteChoice>, pub user_default: Option<String> }
pub fn choose_route(sources: &RouteSources) -> Option<String>;   // account id, or None = built in
```

```proto
// session.proto StartSessionRequest
optional string provider_account_id = 41;   // "" / absent = no session-level choice; "builtin" = explicit built-in
// catalog.proto
rpc ListProviderRoutes(ListProviderRoutesRequest) returns (ListProviderRoutesResponse);
message ListProviderRoutesRequest { string session_token = 1; string harness = 2; string daemon_instance_id = 3; }
message ProviderRouteOption { string account_id = 1; string label = 2; string preset = 3; bool supported = 4; string reason = 5; }
message ListProviderRoutesResponse { repeated ProviderRouteOption options = 1; bool harness_routable = 2; string harness_reason = 3; }
```

```rust
// tddy-changeset SessionMetadata (additive)
#[serde(default, skip_serializing_if = "Option::is_none")] pub provider_account_id: Option<String>,
// tddy-agent-launch SessionAccountAccess
pub fn provider_account(&self, account_id: &str) -> Result<ProviderAccount, SessionIdentityRefusal>;
```

Bodies `todo!()` until green. Field number 41 is checked against `session.proto` in wave 2.

## Green wave

**Wave:** 2 of 5
**Greenable independently:** no — `claude_cli_on_a_meta_account_*` reads a provider account through node 1's codec; resolver/matrix/precedence unit tests are greenable on their own.
**Concurrent with:** none (only node in wave 2)
**Blocks:** `opencode-cli` (3/9), `tool-route` (4/9), `default-route` (5/9) — all consume the resolver and the start field.

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-service` (session, catalog protos), `tddy-agent-backend` (route module), `tddy-agent-launch`
(claude-cli start/resume, vault read), `tddy-changeset` (`.session.yaml`), `tddy-daemon-rpc`
(catalog handler), `tddy-daemon` (wiring), `tddy-web` (Create-session pane, generated code).

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-claude-route.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-claude-route.md)

## Summary

The resolver turns (harness, account, model) into the process environment Claude Code needs; claude-cli
starts and resumes apply it; the pane lets the operator choose.

## Scope

- [ ] Route module: harness ids, matrix for all harnesses, Claude route bodies, precedence
- [ ] `provider_account_id` on start + `.session.yaml` + resume
- [ ] `SessionAccountAccess::provider_account` (vault read, same refusals as GitHub)
- [ ] claude-cli unsandboxed start: route env + secret env into PTY `env_extra`; a sandboxed start with a route is refused (`not yet supported in a sandbox`) until node 9
- [ ] `ListProviderRoutes` catalog RPC
- [ ] Web `CreateSessionProviderField` for claude-cli; disabled for cursor-cli with the reason; model list from `ListProviderAccountModels`

## Technical Changes

### State A
- `StartSessionRequest` has `agent`, `session_type`, `model`, `sandbox` … no provider field (next free 41).
- claude-cli unsandboxed: `claude_cli_spawn_steps.rs` builds `env_extra = git_environment + managed + semantic`; PTY inherits daemon env minus GitHub tokens, plus `spec.env`.
- `.session.yaml` (`deny_unknown_fields`) persists `session_type`, `model`, `agent`…; resume dispatches on `session_type`.
- `SessionAccountAccess` reads the owner's vault for GitHub identity at start.
- No `ANTHROPIC_*` anywhere in the code.

### State B
A claude-cli session started with an account runs with `ANTHROPIC_BASE_URL=<anthropic root>`,
`ANTHROPIC_AUTH_TOKEN=<key>`, `ANTHROPIC_MODEL`, `ANTHROPIC_DEFAULT_OPUS_MODEL`,
`ANTHROPIC_DEFAULT_SONNET_MODEL`, `ANTHROPIC_DEFAULT_HAIKU_MODEL`, `CLAUDE_CODE_SUBAGENT_MODEL` = the
chosen model; `--model` is the same model; the account id is persisted and reapplied on resume.

### Delta
As Scope.

## Implementation Milestones
- [ ] M1 Route module + unit tests
- [ ] M2 Start/resume + metadata + vault read
- [ ] M3 Catalog RPC + web picker

## Testing Plan

Pure resolver tests (matrix, env shape, precedence). Launch-level acceptance: the claude-cli launch env
for a session on a fake vault holding a Meta account (no real CLI). Cypress component test for the picker.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `claude_code_on_meta_gets_the_messages_root_the_key_and_every_alias_pinned` | `packages/tddy-agent-backend/tests/provider_route_acceptance.rs` (new) | env keys/values; key only in `secret_env` |
| `claude_code_refuses_an_account_without_anthropic_messages` | same | Baseten → `ProtocolNotServed` |
| `cursor_cannot_be_routed_whatever_the_account` | same | `HarnessCannotBeRouted` |
| `a_session_choice_beats_an_agent_definition_which_beats_a_default` | same | `choose_route` precedence, incl. explicit built-in |
| `a_claude_cli_start_on_a_meta_account_launches_with_the_route_env` | `packages/tddy-agent-launch/tests/claude_cli_provider_route_acceptance.rs` (new) | launch env from vault account; `--model` |
| `a_locked_vault_refuses_a_routed_start_before_anything_spawns` | same | refusal text; no spawn |
| `a_resumed_claude_cli_session_reapplies_its_account` | same | `.session.yaml` → resume env |
| `session_metadata_round_trips_the_provider_account_id` | `packages/tddy-changeset/src/session_metadata.rs` (unit) | additive field; legacy file parses |
| `list_provider_routes_marks_each_account_for_the_harness` | `packages/tddy-daemon-rpc/tests/provider_routes_catalog.rs` (new) | supported + reason per account |
| `choosing_an_account_lists_its_models_and_sends_it_on_create` | `packages/tddy-web/cypress/component/CreateSessionProviderAcceptance.cy.tsx` (new) | picker; cursor-cli disabled with reason |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Matrix owned once, here**: later nodes replace their harness's `NotYetRoutable` arm; no node adds a second matrix.
- **Precedence owned here** with all three levels, so nodes 5 and 8 only feed sources (no edge 5 → 8).
- **Key in the PTY env**: the harness CLI reads its key from env; unlike the GitHub token, there is no per-call socket it could use.
- **No fallback**: a routed start never silently runs on Anthropic.

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

`feature/provider-route/opencode-cli` (3/9), `feature/provider-route/tool-route` (4/9), `feature/provider-route/default-route` (5/9).

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-claude-route-initial-discovery.md`)
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
