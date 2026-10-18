# Changeset: Provider accounts — connect an API-key account and list its models

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: #provider-route 1/9. Base: `master`. PR: [#662](https://github.com/uppin/tddy-coder/pull/662).

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-18-provider-route-accounts-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Item | Verdict | Note |
|---|---|---|
| [2026-10-04-keyring-accounts-rename-lost-update.md](../todo/2026-10-04-keyring-accounts-rename-lost-update.md) | ⚠ DURING | Relabelling a provider account goes through the same `set_label` read-then-write. This node adds no second read-modify-write path; `connect` is a single `put` of a new record. |
| `packages/tddy-accounts`, `packages/tddy-credentials` have no `docs/code-issues/` | ℹ | Unanalyzed, not clean. |

## Responsibility

Model-provider **accounts** in the vault: the preset table, the record codec, connecting by key with a
model-list probe, listing an account's models, and the Accounts-screen form. Owns
`tddy_accounts::provider_account::{ProviderPreset, WireProtocol, ProviderEndpoints, ProviderAccount,
PRESETS, preset}`, `AccountStore::put_provider_account`, the `ProviderModelProbe` port and its daemon
implementation, the two RPCs and the `AccountSummary` fields below, and the web `ConnectProviderAccountForm`.

## Boundaries

- Does not use an account for any session or harness (node 2 onward).
- Does not touch the model registry (`models.db`, `ModelRegistryService`).
- Does not add defaults (node 5) or edit a stored key in place.
- Does not change the GitHub link flow.

## Dependencies

None — root node.

## Draft PR contract

```proto
// accounts.proto — AccountsService
rpc ConnectProviderAccount(ConnectProviderAccountRequest) returns (ConnectProviderAccountResponse);
rpc ListProviderAccountModels(ListProviderAccountModelsRequest) returns (ListProviderAccountModelsResponse);

message ConnectProviderAccountRequest {
  string session_token = 1;
  string preset = 2;              // "meta" | "fireworks" | "baseten" | "modal" | "openai-compatible" | "anthropic-compatible"
  string label = 3;
  string api_key = 4;             // inbound only
  string anthropic_base_url = 5;  // only for presets whose URL is operator-supplied
  string openai_base_url = 6;     // idem (Responses and Chat share one OpenAI root)
}
message ConnectProviderAccountResponse { AccountSummary account = 1; }
message ListProviderAccountModelsRequest { string session_token = 1; string account_id = 2; }
message ListProviderAccountModelsResponse { repeated string model_ids = 1; }

// AccountSummary (additive)
string preset = 8;
repeated string wire_protocols = 9;   // "anthropic-messages" | "openai-responses" | "openai-chat"
```

```rust
// tddy-accounts/src/provider_account.rs
pub enum WireProtocol { AnthropicMessages, OpenAiResponses, OpenAiChat }
pub struct ProviderEndpoints { pub anthropic_messages: Option<String>, pub openai_responses: Option<String>, pub openai_chat: Option<String> }
pub struct ProviderPreset { pub id: &'static str, pub label: &'static str, /* fixed URLs or "operator-supplied" per protocol */ }
pub fn preset(id: &str) -> Option<&'static ProviderPreset>;
pub fn endpoints_for(preset: &ProviderPreset, anthropic_base_url: Option<&str>, openai_base_url: Option<&str>) -> Result<ProviderEndpoints, ProviderAccountError>;
pub struct ProviderAccount { pub account: AccountId, pub preset: &'static ProviderPreset, pub label: String, pub endpoints: ProviderEndpoints, pub api_key: SecretString }
impl ProviderAccount {
    pub fn to_record(&self, now: u64) -> CredentialRecord;
    pub fn from_record(record: &CredentialRecord) -> Option<ProviderAccount>;   // None for non-provider records (github…)
    pub fn wire_protocols(&self) -> Vec<WireProtocol>;
}
#[async_trait] pub trait ProviderModelProbe: Send + Sync {
    async fn list_models(&self, endpoints: &ProviderEndpoints, api_key: &SecretString) -> Result<Vec<String>, String>;
}
// AccountStore gains:
fn put_provider_account(&self, session_token: &str, account: ProviderAccount) -> Result<CredentialRecord, AccountsError>;
```

Bodies `todo!()` until green; the failing tests below are published with the surface. Exact module
names may be adjusted in wave 2 against HEAD; the RPC and type names above are the contract.

## Green wave

**Wave:** 1 of 5
**Greenable independently:** yes — root node; tests use an in-memory store and a fake probe.
**Concurrent with:** none (only node in wave 1)
**Blocks:** `feature/provider-route/claude-route` (reads `ProviderAccount` from the vault), and transitively every node above.

Real dependency edges, as opposed to the branch line:

    n1 → n2      n2 → n3, n4, n5      n3 → n6, n7, n9      n4 → n7, n8      n6 → n9

## Affected Packages

- [`tddy-service`](../../../packages/tddy-service/README.md) — `accounts.proto`
- [`tddy-accounts`](../../../packages/tddy-accounts/README.md) — presets, codec, service, store port
- [`tddy-daemon`](../../../packages/tddy-daemon/README.md) — probe implementation, wiring
- [`tddy-web`](../../../packages/tddy-web/README.md) — Accounts screen form, generated `accounts_pb.ts`, RPC fake

## Related Feature Documentation

- PRD: [PRD-2026-10-18-provider-route-accounts.md](../../ft/daemon/1-WIP/PRD-2026-10-18-provider-route-accounts.md)

## Summary

A person pastes a provider API key; the daemon validates it by listing the endpoint's models, seals it
in their vault as a provider-account record, and the Accounts screen shows it under its provider.

## Scope

- [ ] Preset table (six presets, URLs per protocol) and `endpoints_for` validation
- [ ] `ProviderAccount` ⇄ `CredentialRecord` codec (metadata keys `preset`, `endpoint.<protocol>`, `probed_at`)
- [ ] `AccountStore::put_provider_account` (vault `put`, refusals as today)
- [ ] `ConnectProviderAccount`: validate → probe → store; `ListProviderAccountModels`
- [ ] `AccountSummary.preset` / `wire_protocols`
- [ ] Daemon `HttpProviderModelProbe` (`GET <openai root>/models` with bearer; Anthropic-only presets `GET <anthropic root>/v1/models` with `x-api-key`)
- [ ] Web: "Connect API key" form, provider groups, model list on expand

## Technical Changes

### State A
- `accounts.proto` `AccountsService` = `ListAccounts`, `SetAccountLabel`, `RemoveAccount`,
  `BeginLinkAccount`, `PollLinkAccount`; `AccountSummary` fields 1–7; `provider` is an open string.
- `CredentialRecord { provider, account, label, secret: SecretString, metadata: BTreeMap, updated_at, version }`;
  `SessionVault::{put,get,list,remove}`; `SessionVaultAccountStore::vault_for(session_token)` refuses
  Locked / Uninitialized / NoSuchSession.
- Accounts are added only by GitHub device link (`AccountLinker`, `LinkError::UnsupportedProvider`).
- Model registry `openai_compatible.rs` appends `/v1/models` to a root (would double `/v1` for Meta).
- Web `AccountsScreen.tsx` "Add account" → `useLinkFlow.ts` `beginLinkAccount`.

### State B
The vault holds provider-account records beside GitHub ones; `ListAccounts` groups them by preset id;
`ConnectProviderAccount` adds one after a successful probe; `ListProviderAccountModels` lists models.

### Delta
As Scope. `tddy-accounts` does not depend on `tddy-model-registry`; the probe is a port implemented in
the daemon with the HTTP client the daemon already links.

## Implementation Milestones
- [ ] M1 Presets + codec + unit tests
- [ ] M2 Service RPCs over an in-memory store and fake probe
- [ ] M3 Daemon probe + wiring; web form

## Testing Plan

Acceptance at the service level (in-memory `AccountStore`, fake `ProviderModelProbe`), unit tests for
presets and the codec, a daemon-level probe test against a local HTTP stub, and a Cypress component
test for the form using `mountWithRpc` + `anInMemoryRpcBackend`.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `connecting_a_meta_account_stores_a_sealed_record_listed_under_meta` | `packages/tddy-accounts/tests/provider_accounts_acceptance.rs` (new) | probe ok → record stored, `ListAccounts` shows preset/protocols, no key |
| `a_key_the_provider_rejects_is_refused_and_nothing_is_stored` | same | probe error → `INVALID_ARGUMENT` with the provider's reason; store unchanged |
| `a_preset_with_an_operator_supplied_url_requires_one` | same | modal / generic without URL refused |
| `a_fixed_preset_refuses_a_url_override` | same | meta + base URL refused |
| `a_locked_vault_refuses_the_connect` | same | `FAILED_PRECONDITION` vault locked |
| `listing_an_accounts_models_asks_its_endpoint_with_its_key` | same | `ListProviderAccountModels` → probe called with that account's endpoints and key |
| `meta_preset_serves_messages_responses_and_chat_at_its_documented_roots` | `packages/tddy-accounts/src/provider_account.rs` (unit) | preset table |
| `a_provider_account_round_trips_through_a_credential_record` | same | codec; `from_record` is `None` for a GitHub record |
| `the_http_probe_lists_models_from_the_openai_root_without_doubling_v1` | `packages/tddy-daemon/tests/provider_model_probe.rs` (new) | `…/v1` + `/models`, bearer header |
| `connecting_an_api_key_account_adds_it_to_its_provider_group` | `packages/tddy-web/cypress/component/ProviderAccountConnectAcceptance.cy.tsx` (new) | form → RPC → list |

## Technical Debt & Production Readiness

- **Draft-PR contract state (2026-10-18):** every owned body is `todo!()` / `TODO(accounts)` and the web
  form renders nothing; `summary_of` fills the new `AccountSummary` fields with defaults. None of this
  may merge — `/green` implements it in this PR.
- The existing `accounts_stub_daemon_acceptance.rs` assertion gained the two new (empty) fields — a
  compile consequence of the additive proto change, not a behaviour change.

## Decisions & Trade-offs
- **Vault, not registry** (developer decision 2026-10-18): sealed, per person, syncable; costs a second
  provider concept beside the registry and an unlocked vault at use time.
- **Probe before store**: a key that cannot list models is not stored — a typo is caught at connect,
  not at session start.
- **One OpenAI root** per account serves both Responses and Chat (`…/v1`); Anthropic root kept separately.

## Refactoring Needed
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Red-phase record (wave 2, 2026-10-18)

Scoped verification — `tddy-accounts`, `tddy-daemon` (Rust), one Cypress spec (web):

- `cargo clippy --all-targets -p tddy-accounts -p tddy-daemon -- -D warnings` — clean on the published surface.
- `provider_accounts_acceptance` — 8/8 fail, each at `service.rs` `todo!()` (this node).
- `provider_account::tests` — 9/9 fail at this node's `todo!()`.
- `provider_model_probe` — 3/3 fail at `provider_model_probe.rs` `todo!()`.
- `ProviderAccountConnectAcceptance.cy.tsx` — 4/4 fail: `[data-testid='accounts-connect-api-key']` never rendered.
- Baseline caveat: the pre-edit baseline build overlapped the surface edits, so no clean pre-edit build of
  these packages was captured; master is green on CI at `515c00149`.

## Validation Results
### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## Successor PRs

`feature/provider-route/claude-route` (#provider-route 2/9).

## TODO

- [x] Record initial discovery (`2026-10-18-provider-route-accounts-initial-discovery.md`)
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
