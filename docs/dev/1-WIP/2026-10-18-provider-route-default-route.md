# Changeset: Per-person default provider account per harness

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 5/9. Base: `feature/provider-route/tool-route`.

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
// tddy-accounts
pub fn default_account_for(held: &[CredentialRecord], harness: &str) -> Option<AccountId>;
fn set_provider_account_defaults(&self, session_token: &str, account: &AccountId, harnesses: &[String]) -> Result<Vec<CredentialRecord>, AccountsError>; // AccountStore
```

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

- [ ] RPC + store method + summary field
- [ ] One default per harness per person (clear previous)
- [ ] Refuse a harness the account cannot drive
- [ ] Start feeds `user_default`
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
| `a_claude_cli_start_without_an_account_uses_the_persons_default` | `packages/tddy-agent-launch/tests/claude_cli_provider_route_acceptance.rs` | default fed |
| `an_explicit_built_in_choice_overrides_the_default` | same | precedence via node 2 |
| `setting_a_second_default_for_a_harness_clears_the_first` | `packages/tddy-accounts/tests/provider_account_defaults_acceptance.rs` (new) | one holder |
| `a_default_the_account_cannot_drive_is_refused` | same | matrix check |
| `marking_an_account_default_shows_on_its_row_and_preselects_it` | `packages/tddy-web/cypress/component/ProviderAccountDefaultsAcceptance.cy.tsx` (new) | UI |

## Technical Debt & Production Readiness

## Decisions & Trade-offs
- **Per person, in the vault record** — survives restarts and syncs with the account; a daemon default
  cannot exist without a daemon-held key.

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

- [x] Record initial discovery (`2026-10-18-provider-route-default-route-initial-discovery.md`)
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
