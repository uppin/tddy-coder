# Changeset: Claude CLI sessions on a provider account — the route resolver

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — seven packages (tddy-accounts, tddy-agent-backend, tddy-agent-launch, tddy-changeset, tddy-daemon, tddy-service, tddy-web), proto contract change
**Stack**: #provider-route 2/9. Base: `feature/provider-route/accounts`. PR: [#663](https://github.com/uppin/tddy-coder/pull/663).

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
`StartSessionRequest.{provider_account_id, provider_built_in}`, the per-session record
`tddy_changeset::session_provider_account` (`provider-account.yaml`), `accounts.ListProviderRoutes`
with its `tddy_accounts::RouteSupport` port and the daemon's `HarnessRouteSupport`, the claude-cli
start/resume application (`tddy_agent_launch::session_provider_route`), the vault read
`SessionAccountAccess::provider_account`, and the web `CreateSessionProviderField` plus
`CreateSessionPaneProps.accountsClient`.

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
string provider_account_id = 41;   // "" = no choice at this level
bool provider_built_in = 42;       // explicitly the vendor; overrides agent definition and default
// accounts.proto AccountsService (served by tddy-accounts; the matrix comes in through RouteSupport)
rpc ListProviderRoutes(ListProviderRoutesRequest) returns (ListProviderRoutesResponse);
message ListProviderRoutesRequest { string session_token = 1; string harness = 2; }
message ProviderRouteOption { string account_id = 1; string label = 2; string preset = 3; bool supported = 4; string reason = 5; }
message ListProviderRoutesResponse { repeated ProviderRouteOption options = 1; bool harness_routable = 2; string harness_reason = 3; }
```

```rust
// tddy-accounts
pub trait RouteSupport: Send + Sync {
    fn harness_routable(&self, harness: &str) -> Result<(), String>;
    fn supports(&self, harness: &str, preset: &str, protocols: &[&str]) -> Result<(), String>;
}
impl AccountsServiceImpl<S> { pub fn with_route_support(self, support: Arc<dyn RouteSupport>) -> Self; }
// tddy-daemon
pub struct HarnessRouteSupport;   // RouteSupport over tddy_core::provider_route
// tddy-changeset — beside .session.yaml, not in it (deny_unknown_fields; ~48 struct literals)
pub const SESSION_PROVIDER_ACCOUNT_FILENAME: &str = "provider-account.yaml";
pub struct SessionProviderAccount { pub account_id: String, pub model: String }
pub fn write_session_provider_account(session_dir: &Path, account: &SessionProviderAccount) -> Result<(), WorkflowError>;
pub fn read_session_provider_account(session_dir: &Path) -> Result<Option<SessionProviderAccount>, WorkflowError>;
// tddy-agent-launch
impl SessionAccountAccess { pub fn provider_account(&self, account_id: &str) -> Result<ProviderAccount, SessionIdentityRefusal>; }
pub fn session_route_sources(req: &StartSessionRequest) -> RouteSources;
pub fn resolve_session_route(access: &SessionAccountAccess, harness: Harness, sources: &RouteSources, model: &str) -> Result<Option<HarnessRoute>, Status>;
pub fn pty_route_env(route: &HarnessRoute) -> Vec<(String, String)>;
pub fn route_sources_on_resume(session_dir: &Path) -> Result<(RouteSources, Option<String>), Status>;
```

```ts
// tddy-web
export interface CreateSessionProviderFieldProps { harnessRoutable; harnessReason; choices: ProviderRouteChoice[]; value: string /* "builtin" | account id */; onChange }
// CreateSessionPaneProps.accountsClient?: Client<typeof AccountsService>
```

Bodies `todo!()` until green. Field numbers 41/42 verified free in `StartSessionRequest` (max was 40; 19–23 reserved).

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
| `claude_code_on_meta_gets_the_messages_root_the_key_and_every_alias_pinned` | `packages/tddy-agent-backend/tests/provider_route_acceptance.rs` (new) | env keys/values for claude, claude-acp, claude-cli; key only in `secret_env`, never env or args |
| `claude_code_refuses_an_account_without_anthropic_messages` | same | Baseten → `ProtocolNotServed`, message names Anthropic Messages |
| `cursor_cannot_be_routed_whatever_the_account` | same | `HarnessCannotBeRouted` for cursor and cursor-cli |
| `the_matrix_answers_without_building_a_route` | same | `route_supported` |
| `every_harness_id_names_its_harness_and_back` | same | ten ids round-trip; `stub`, `tool` are none |
| `a_session_choice_beats_an_agent_definition_which_beats_a_default` | same | `choose_route` |
| `an_explicit_built_in_choice_overrides_every_lower_level` | same | `RouteChoice::BuiltIn` |
| `a_route_can_be_logged_without_its_key` | same | redacted `Debug` |
| `list_provider_routes_marks_each_account_for_the_harness` | `packages/tddy-accounts/tests/provider_routes_acceptance.rs` (new) | provider accounts only, vault order, matrix verdicts verbatim |
| `a_harness_that_cannot_be_routed_says_so_once_and_marks_every_account` | same | Cursor |
| `a_daemon_without_the_matrix_refuses_rather_than_guessing` | same | unwired → `FAILED_PRECONDITION` |
| `a_session_on_its_vendor_reads_as_no_account` / `a_recorded_account_reads_back_as_written` / `the_record_holds_no_secret_shaped_field` | `packages/tddy-changeset/src/session_provider_account.rs` (unit) | per-session record |
| `a_claude_cli_start_on_a_meta_account_launches_with_the_route_env` | `packages/tddy-agent-launch/src/session_provider_route_tests.rs` (new) | real unlocked vault → PTY env |
| `a_start_that_chooses_nothing_runs_on_the_vendor` | same | `None` |
| `a_locked_vault_refuses_a_routed_start_before_anything_spawns` | same | `FAILED_PRECONDITION` "locked" |
| `an_account_the_vault_does_not_hold_is_refused` | same | `NOT_FOUND` |
| `a_harness_that_cannot_run_on_the_account_is_refused_with_the_reason` | same | `INVALID_ARGUMENT` naming Cursor |
| `a_start_request_names_its_session_level_choice` | same | `provider_account_id` / `provider_built_in` → `RouteSources.session` |
| `a_resumed_claude_cli_session_reapplies_its_account` / `a_resumed_session_with_no_record_runs_on_the_vendor` | same | resume via `provider-account.yaml` |
| Cypress: defaults to built-in / lists the account's models / sends the account and model / Cursor shows why | `packages/tddy-web/cypress/component/CreateSessionProviderAcceptance.cy.tsx` (new) | picker |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Matrix owned once, here**: later nodes replace their harness's `NotYetRoutable` arm; no node adds a second matrix.
- **`ListProviderRoutes` on `AccountsService`, not `CatalogService`** (changed in wave 2): the catalog is
  defined as answered from configuration rows, not daemon state, and this answer needs the vault. The
  matrix reaches `tddy-accounts` through the `RouteSupport` port, so that crate does not depend on
  the harness crates.
- **Per-session record beside `.session.yaml`** (changed in wave 2): a `SessionMetadata` field would
  touch ~48 struct literals across the workspace; `provider-account.yaml` follows the
  `github-source.yaml` precedent and holds the account id and model only.
- **Precedence owned here** with all three levels, so nodes 5 and 8 only feed sources (no edge 5 → 8).
- **Key in the PTY env**: the harness CLI reads its key from env; unlike the GitHub token, there is no per-call socket it could use.
- **No fallback**: a routed start never silently runs on Anthropic.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped verification — `tddy-agent-backend`, `tddy-accounts`, `tddy-changeset`, `tddy-agent-launch`,
`tddy-daemon` (Rust); one Cypress spec (web):

- `cargo clippy --all-targets -p tddy-agent-backend -p tddy-accounts -p tddy-changeset -p tddy-agent-launch -p tddy-daemon -- -D warnings` — clean.
- `provider_route_acceptance` 8/8 fail at `provider_route.rs` `todo!()` (this node).
- `session_provider_account` 3/3 fail at this node's `todo!()`.
- `provider_routes_acceptance` 3/3 and five of `session_provider_route_tests` fail **first at
  `tddy-accounts/src/provider_account.rs` `preset()`** — node 1's codec, which these tests write real
  records through. Sequencing fact: they turn green only after #662 is green. The other three
  `session_provider_route_tests` fail at this node's `todo!()`.
- `CreateSessionProviderAcceptance.cy.tsx` 4/4 fail: the picker is not rendered.

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
