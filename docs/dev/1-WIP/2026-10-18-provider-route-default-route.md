# Changeset: Per-person default provider account per harness

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4/5
**Stack**: #provider-route 5/9. Base: `feature/provider-route/tool-route`. PR: [#666](https://github.com/uppin/tddy-coder/pull/666).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-default-route-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-04-keyring-accounts-rename-lost-update.md](../todo/2026-10-04-keyring-accounts-rename-lost-update.md) | ⚠ DURING | Setting a default rewrites up to two records (new holder, previous holder). Do it under one vault operation per record and do not copy `set_label`'s two-step read-then-write without the same TODO; prefer a vault-level read-modify-write if wave 2 finds one. |

## Responsibility

The **user-default** level of route precedence. Owns `AccountsService.SetProviderAccountDefaults`,
`AccountSummary.default_for` (repeated harness ids), the record metadata key `default_for`, the vault
read that feeds `RouteSources.user_default` at start, the Accounts-screen "Default for…" control, and the
pane's pre-selection.

## Boundaries

- Does not change precedence (node 2 owns `choose_route`) or the matrix.
- Does not add a daemon-wide default (impossible: no daemon-held key).
- Does not add agent-definition routes (node 8).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/provider-route/accounts` (1/9) | provider-account records, codec, `AccountsService` | stores `default_for` in the record metadata via the codec | change the codec's other keys |
| `feature/provider-route/claude-route` (2/9) | `RouteSources`, `choose_route`, `route_supported`, start-time vault read | fills `user_default`; validates a default with `route_supported` | edit `choose_route` |
| `feature/provider-route/tool-route` (4/9) | tool-session route application | defaults apply to tool sessions through it | — |

## Draft PR contract

```proto
rpc SetProviderAccountDefaults(SetProviderAccountDefaultsRequest) returns (SetProviderAccountDefaultsResponse);
message SetProviderAccountDefaultsRequest { string session_token = 1; string account_id = 2; repeated string harnesses = 3; }
message SetProviderAccountDefaultsResponse { repeated AccountSummary changed = 1; }
// AccountSummary
repeated string default_for = 10;
```

```rust
// tddy-accounts (provider_account.rs)
pub const META_DEFAULT_FOR: &str = "default_for";       // comma-separated harness ids in the record metadata
pub fn default_account_for(held: &[CredentialRecord], harness: &str) -> Option<AccountId>;
pub fn defaults_of(record: &CredentialRecord) -> Vec<String>;
pub trait ProviderAccountDefaults: Send + Sync {          // a new port: node 1's ProviderAccountStore is not widened
    fn set_provider_account_defaults(&self, session_token: &str, account: &AccountId, harnesses: &[String]) -> Result<Vec<CredentialRecord>, AccountsError>;
}
impl ProviderAccountDefaults for SessionVaultAccountStore;
impl AccountsServiceImpl<S> { pub fn with_provider_account_defaults(self, defaults: Arc<dyn ProviderAccountDefaults>) -> Self; }
// tddy-agent-launch
impl SessionAccountAccess { pub fn default_provider_account(&self, harness: &str) -> Result<Option<String>, SessionIdentityRefusal>; }
```

The start path feeds `RouteSources.user_default` from `default_provider_account` (node 2's `choose_route` decides).

## Green wave

**Wave:** 3 of 5
**Greenable independently:** no — the start test needs node 2's start path.
**Concurrent with:** `opencode-cli` (3/9), `tool-route` (4/9)
**Blocks:** none

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

`tddy-service` (accounts.proto), `tddy-accounts`, `tddy-agent-launch` (source feed), `tddy-web`.

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-default-route.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-default-route.md)

## Scope

- [x] RPC + store method + summary field
- [x] One default per harness per person (clear previous)
- [x] Refuse a harness the account cannot drive
- [x] Start feeds `user_default`
- [ ] Accounts screen control; pane pre-selection

## Technical Changes

### State A
After node 2, `RouteSources.user_default` exists and is always `None`.

### State B
The owner's vault records carry `default_for`; starts without a session choice use it.

## Testing Plan

Service acceptance over an in-memory store; launch acceptance for the start; Cypress for the control.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `setting_a_second_default_for_a_harness_clears_the_first` / `a_default_the_account_cannot_drive_is_refused` / `an_accounts_defaults_are_listed_on_its_row` / `no_account_is_the_default_for_a_harness_nobody_chose` | `packages/tddy-accounts/tests/provider_account_defaults_acceptance.rs` (new) | RPC + metadata |
| `a_claude_cli_start_without_an_account_uses_the_persons_default` / `an_explicit_built_in_choice_overrides_the_default` / `a_harness_with_no_default_has_none` | `packages/tddy-agent-launch/src/default_route_tests.rs` (new) | real vault, start-time feed |
| Cypress: marks the Claude CLI default from the row / pre-selects it on Create | `packages/tddy-web/cypress/component/ProviderAccountDefaultsAcceptance.cy.tsx` (new) | UI |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **A new `ProviderAccountDefaults` port** (wave 2) rather than a method on node 1's `ProviderAccountStore`, so this node does not change a parent's trait.
- **Open question for /green (ask the developer):** a start that names no account while the owner's vault is **locked** cannot read the default. Refusing blocks every start for a person with a locked vault; proceeding on the vendor is a fallback the house rules forbid without consent. Not pinned by a test yet.
- **Per person, in the vault record** — survives restarts and syncs with the account; a daemon default
  cannot exist without a daemon-held key.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped: `tddy-accounts`, `tddy-agent-launch`, `tddy-daemon` — clippy `-D warnings` clean (the stub-daemon assertion gained the empty `default_for` field).

- `provider_account_defaults_acceptance` 4/4 and `default_route_tests` 3/3 fail first at node 1's `preset()` (their fixtures build real records) — green after #662; then at this node's `todo!()`.
- `ProviderAccountDefaultsAcceptance.cy.tsx` 2/2 fail: no default-for control; no picker (node 2).

## Green-phase record (2026-10-22)

Scoped: `tddy-accounts` (all targets pass, incl. the 4 acceptance tests and a vault-store test), `tddy-agent-launch` (`default_route_tests` 3/3; 4 `tool_session_route_tests` still red — node 4's `todo!()`, not this node's), clippy `-D warnings` clean for `tddy-accounts`, `tddy-agent-launch`, `tddy-daemon`.

- A locked vault refuses a start that names no account (`session_route_sources_with_default`); an absent or uninitialised vault has no default. Open question above is answered conservatively, pending the developer.
- The default feeds the unsandboxed `claude-cli` and `opencode-cli` starts. Sandboxed starts, tool sessions and resume still read session-level choice only.
- TODO(default-route): Accounts-screen control and pane pre-selection (Cypress) not yet implemented.

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-default-route-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests (per-node gate waived by the developer on 2026-10-18 — "finish remaining waves without stopping"; review at stack hand-off)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run scoped tests for touched packages; full-workspace result from CI
- [ ] Validate changes (/validate-changes)
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (scoped)
- [ ] Wrap documentation (/wrap-context-docs)
- [ ] USER REVIEW — work complete
