# Changeset: Accounts service and screen

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#keyring` 4/9 · branch `feature/keyring/accounts` · base `feature/keyring/store` (#510)

## Affected Packages

- **tddy-service**: `proto/accounts.proto` — **new**; generated Rust and TS
- **tddy-accounts** (**new crate**): `docs/accounts-service.md` — the service over `SessionVault`
- **tddy-daemon**: [daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md)
  - `src/runtime.rs` — one `rpc_entries.push(...)` beside the existing ones
- **tddy-web**: [capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md),
  [projects-screen.md](../../../packages/tddy-web/docs/projects-screen.md)
  - `src/routing/appRoutes.ts`, `src/index.tsx`, `src/components/shell/DaemonNavMenu.tsx`,
    `src/components/accounts/` (new)

## Related Feature Documentation

- [PRD — Accounts screen and service](../../ft/web/1-WIP/PRD-2026-09-19-keyring-accounts.md)
- [App shell](../../ft/web/app-shell.md)
- [Projects screen — multi-host](../../ft/web/projects-screen-multi-host.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)

## Summary

Adds the first surface over the credential store `#keyring` 3/9 introduced: `accounts.proto`, an
`AccountsService` that lists and curates records **without ever returning a secret**, and an
`/accounts` screen in `tddy-web`.

## Background

`tddy-web` already has eight daemon-scoped screens, and the pattern each follows is four small
pieces: a route constant and an `is*Path` predicate in `src/routing/appRoutes.ts:88-130`, an
`*AppPage` component, a nav `Button` with a `data-testid` in `DaemonNavMenu.tsx:55-140`, and one rung
in the ladder at `src/index.tsx:483-499`. This node adds a ninth by the same four pieces — the work
is the service and the three list states, not the plumbing.

After 3/9 the vault's contents are observable by nothing. That blocks 5/9, whose assignment UI must
show what is assignable, and it leaves the deliberate `Locked` outcome with nowhere to be reported —
so its only symptom would be git operations failing later, unexplained.

## Responsibility

**This node owns how a person sees and curates what is in the vault.**

- `accounts.proto` and the `AccountsService` contract;
- the rule, enforced by message shape, that no response carries a secret;
- the three distinct list outcomes — empty, `vault_locked`, error;
- the `/accounts` screen, its route, its nav entry and its rendering of those three states.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| `AccountsService` — `ListAccounts`, `SetAccountLabel`, `RemoveAccount` | `tddy-service` (proto) |
| `AccountSummary` | `tddy-service` (proto) |
| the service implementation and its entry constructor | `tddy-accounts` (new) |
| `ACCOUNTS_ROUTE`, `isAccountsPath` | `tddy-web` |
| `AccountsAppPage` and the accounts list | `tddy-web` |

**Explicitly not this node's:**

- **The vault itself** — `#keyring` 3/9. This node calls `SessionVault` and adds nothing to it.
- **Assigning an account to a project** — `#keyring` 5/9.
- **Propagating the vault between daemons** — `#keyring` 6/9.
- **Screen-sharing as a provider** — `#keyring` 7/9.
- **Linking an account** — each provider's flow is its own node (GitHub: 8/9). This screen shows and
  curates what exists; it does not create.

**Two lines this node must not cross:**

1. **No secret in any response.** There is no `GetAccount` returning a credential and no secret field
   on `AccountSummary`; `has_secret` is one bit. 3/9 made this a boundary, and this node is the most
   plausible place to cross it.
2. **No fourth reader of `capabilities`.** Accounts is plain RPC, so the screen is not gated.
   `useHasCapability` remains the single predicate
   ([capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md) § *The one
   predicate*).

**Crate-boundary choice**: the service lives in a new `tddy-accounts`, not in `tddy-credentials`.
3/9 kept the store free of `tddy-service` and of the auth crate so 6/9 and 7/9 can depend on it
cheaply; adding proto and RPC to it would push that cost onto every dependent.

## Dependencies

**Parent in the line**: `#keyring` 3/9 `store` — [#510](https://github.com/uppin/tddy-coder/pull/510).
This is a **real dependency edge**, not just a line position: `SessionVault`, `ProviderId`,
`AccountId` and `VaultError::Locked` are all 3/9's, and this node's three list outcomes are a direct
rendering of them.

**Transitively**: 1/9 [#508](https://github.com/uppin/tddy-coder/pull/508) and 2/9
[#509](https://github.com/uppin/tddy-coder/pull/509) via 3/9.

**Dependents**: 5/9 `assignments`, 6/9 `sync`, 7/9 `screen-share`, 8/9 `link-github`, and 9/9
transitively — **five**, which is why this node is alone in its wave and why nothing else can start
until it lands.

**New external dependencies: none.** The proto toolchain, `tddy-rpc` and the web test harness are
all in place.

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `packages/tddy-service/proto/accounts.proto` — `AccountsService` with its three RPCs and
  `AccountSummary` as specified in the PRD; generated Rust and TS committed.
- `tddy-accounts`: the service struct and its entry constructor, signatures only, bodies `todo!()`.
- `tddy-web`: `ACCOUNTS_ROUTE` and `isAccountsPath` — real, since they are pure string rules and
  their tests are cheap.

**Failing tests**

- `ListAccounts` returns the vault's records grouped by provider;
- **no field of any response carries a secret** — asserted over the serialised response, not the
  struct;
- an empty vault, a `Locked` vault and an I/O error produce three distinct responses;
- an invalid `session_token` is refused rather than served an empty list;
- `SetAccountLabel` changes the label and leaves `account_id` intact;
- `RemoveAccount` removes exactly one record;
- `isAccountsPath` matches `/accounts` and not `/accounts-archive` (the existing predicates' rule);
- Cypress component: the three list states render distinctly; rename and remove call the right RPC.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

### Addition to the drafted contract — the `AccountStore` port

The contract above names only the service struct and its entry constructor. Writing the tests turned
up a fact the plan did not have: **3/9's `SessionVault` cannot be constructed in a test.** Its `path`
field is private and it exposes no constructor — the only way to obtain one is
`CredentialStore::open_or_create`, which needs a real sealed file on disk and input keying material.
A `ListAccounts` test that had to seal a vault first would be testing 3/9's crypto, not this node's
grouping, and it would fail for 3/9's `todo!()` rather than for its own.

So this node owns one more symbol than the plan listed — a **port trait** in
`packages/tddy-accounts/src/store.rs`:

```rust
pub enum AccountsError { NoSuchSession, Locked, Unavailable(String) }

pub trait AccountStore: Send + Sync {
    fn list(&self, session_token: &str) -> Result<Vec<CredentialRecord>, AccountsError>;
    fn set_label(&self, session_token: &str, provider: &ProviderId, account: &AccountId,
                 label: &str) -> Result<CredentialRecord, AccountsError>;
    fn remove(&self, session_token: &str, provider: &ProviderId, account: &AccountId)
        -> Result<(), AccountsError>;
}
```

Three things about it are deliberate:

- **It is expressed over 3/9's real data types** — `CredentialRecord`, `ProviderId`, `AccountId` are
  fully implemented and need no vault to build. Only the *opening* of a vault is stubbed upstream, so
  a fake implementing this trait carries real records.
- **It adds nothing to `SessionVault`.** The boundary rule is that no node implements a symbol
  another owns; a new trait in *this* crate, with 3/9's types as parameters, does not touch 3/9's
  surface. The green phase adds this crate's own implementation over `SessionVault` — again in this
  crate, not in `tddy-credentials`.
- **Its three error variants are exactly the three outcomes the PRD refuses to collapse**, plus the
  refused token. `Locked` becomes `vault_locked: true`, `Unavailable` becomes an RPC error carrying
  the reason meant for the person (an I/O failure's text names server-side paths, so the client gets
  a fixed path-free sentence and the daemon log gets the full error with its subject), `NoSuchSession` becomes a refusal — and an `Ok(vec![])` becomes an empty list. Four
  inputs, four distinguishable responses, which is what the acceptance tests assert.

## Green wave

**Wave 3 of 5 — alone in its wave.**

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

Nothing shares wave 3, so there is no intra-wave sort to make. Five nodes wait directly or
transitively on this one — it is the narrowest point of the stack, and the wave-4 nodes can all
proceed in parallel once it lands.

## Prerequisites

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

This node adds one `rpc_entries.push(...)` line to an already-806-line function, making the recorded
problem marginally worse. Recorded, **not claimed** and **not fixed here**: a mechanical split inside
a feature PR buries the reviewable diff, and doing it in a node five others are waiting on would be
the worst place in the stack to spend that time. 3/9 also records it.

### Unanalyzed packages

`tddy-web` carries code-issue records; none is in this node's path — it adds files rather than
changing the components those records name. `tddy-accounts` is new. `tddy-service` has no
`docs/` directory at all, so nothing has been measured there; **"not measured" is not "clean"**, and
this node does not claim otherwise.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-accounts.md](../../ft/web/1-WIP/PRD-2026-09-19-keyring-accounts.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: proto + surface + failing tests (wave 2, commit 2)
- [ ] **Proto**: `accounts.proto` and its generated code
- [ ] **Service**: `tddy-accounts` over `SessionVault`, three distinct list outcomes
- [ ] **Registration**: one entry in `runtime.rs`
- [ ] **Web**: route, predicate, `AccountsAppPage`, nav entry, ladder rung
- [ ] **Testing**: Rust service tests + Cypress component tests + `appRoutes` unit tests
- [ ] **Package Documentation**: `tddy-accounts`, `tddy-web`
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- The vault exists (3/9) and nothing can observe it.
- Eight daemon-scoped screens; no accounts route, no accounts service, no `accounts.proto`.
- `VaultError::Locked` has no representation above the store.

### State B (Target)

- `AccountsService` lists and curates records, returning summaries and never secrets.
- `/accounts` is the ninth daemon-scoped screen, grouped by provider, with rename and remove.
- Empty, locked and errored are three distinct, explained states from `VaultError` to the DOM.

### Delta (What's Changing)

#### tddy-service
- **API**: `proto/accounts.proto` — `AccountsService` with `ListAccounts`, `SetAccountLabel`,
  `RemoveAccount`; `AccountSummary { provider, account_id, label, subject, updated_at, has_secret }`.
  Every request carries `session_token` first, as every other service does.

#### tddy-accounts (new)
- **Architecture**: depends on `tddy-credentials` and `tddy-service`; **not** on `tddy-daemon-auth`
  and **not** on LiveKit.
- **Implementation**: maps `SessionVault` results to the three outcomes; refuses an invalid
  `session_token` rather than serving an empty list.

#### tddy-daemon
- **Implementation**: one `rpc_entries.push(...)`. **`runtime::build` is not split.**

#### tddy-web
- **Routing**: `ACCOUNTS_ROUTE = "/accounts"` and `isAccountsPath`, with unit tests beside the
  existing ones in `appRoutes.test.ts`.
- **UI**: `src/components/accounts/AccountsAppPage.tsx` and the list; a `shell-menu-accounts` nav
  entry; one rung in `index.tsx`'s ladder.
- **Not changed**: `useHasCapability`, `capabilityAvailability`, and every other screen.

## Implementation Milestones

- [ ] **M1** — `accounts.proto` + generated code
- [ ] **M2** — `tddy-accounts`: `ListAccounts` with the three outcomes distinct
- [ ] **M3** — `SetAccountLabel`, `RemoveAccount`, and `session_token` refusal
- [ ] **M4** — register the entry in `runtime.rs`
- [ ] **M5** — route constant, predicate and their tests
- [ ] **M6** — `AccountsAppPage`, the list, rename and remove
- [ ] **M7** — nav entry + ladder rung
- [ ] **M8** — Cypress component tests and a Storybook story
- [ ] **M9** — `tddy-accounts` and `tddy-web` documentation

## Testing Plan

### Testing Strategy

**Rust for the contract, Cypress for the screen, and the split is where the risk is.** That no
response carries a secret is a property of the wire, so it is asserted in Rust **over the serialised
response** — a struct-field assertion would pass a refactor that adds a field later. That empty,
locked and errored look different to a person is a property of the DOM, so it is a component test.

Cypress component tests use `mountWithRpc` + `anInMemoryRpcBackend`, never `cy.intercept` — the house
rule, and here it also means the three states are produced by a backend returning what the real one
returns rather than by a hand-written fixture.

### Rust tests (`tddy-accounts`)

- `ListAccounts` groups a multi-provider vault by provider.
- **No field of the serialised response contains a stored secret.**
- An empty vault, `VaultError::Locked` and an I/O error produce three distinct responses.
- An invalid or absent `session_token` is refused — **not** served an empty list.
- `SetAccountLabel` changes only the label; `account_id` and the secret are untouched.
- `RemoveAccount` removes exactly one record and leaves the rest openable.

### Web tests

- `appRoutes.test.ts`: `/accounts` matches, `/accounts-archive` does not, `/accounts/x` does not.
- Cypress component: the three list states render distinctly, each naming its reason.
- Cypress component: rename issues `SetAccountLabel`; remove issues `RemoveAccount` only after
  confirmation.

### Verification scope

`./test -p tddy-accounts -p tddy-service -p tddy-daemon` and scoped clippy. For `tddy-web`, **the
single spec and story under change** — a full Cypress e2e run is ~50 minutes over 207 specs and
belongs to CI. `tsc` is not a gate in this repo. Whole-workspace green comes from CI via
`scripts/ci-status.sh`.

### Measured red state (wave 2)

Scoped to the packages this commit touches. `tddy-service` is the only *existing* Rust package it
changes; `tddy-accounts` is new, so it has no baseline of its own.

| Run | Command | Passed | Failed |
|---|---|---|---|
| Baseline, before this commit | `./test -p tddy-service --no-fail-fast` | 134 | 0 |
| After this commit | `./test -p tddy-accounts -p tddy-service --no-fail-fast` | 135 | 9 |
| Web routing | `bun test src/routing` | 98 | 0 |
| Web acceptance | `cypress run --component --spec cypress/component/AccountsScreenAcceptance.cy.tsx` | 0 | 9 |

`tddy-service` is **unchanged at 134 passed, 0 failed** — adding a proto and a module adds surface
and moves nothing. The delta is this node's own: **9 Rust failures and 9 Cypress failures**, with two
tests already green.

- **9 of 10** in `tddy-accounts`' `accounts_service_acceptance.rs`, every one of them on
  `AccountsServiceImpl`'s three `todo!()` bodies, named in the panic.
- **1 of 10 passes**: `the_registry_entry_names_the_service_a_client_addresses`. The generated
  `AccountsServiceServer` and `build_accounts_entry` are surface rather than implementation, so the
  name a client addresses holds from this commit on.
- **9 of 9** in `AccountsScreenAcceptance.cy.tsx`, each on a missing `data-testid` — the screen is
  published as props and a comment, so no row, notice or control exists yet.
- **4 of 4 pass** in `appRoutes.test.ts`. `ACCOUNTS_ROUTE` and `isAccountsPath` are real in this
  commit, as the contract says: they are pure string rules whose tests cost a second.

⚠ **Inherited, not this node's**: regenerating the committed TypeScript surfaced drift in
`packages/tddy-rust-typescript-tests/gen/auth_pb.ts`. `#keyring` 2/9 added `StartDeviceLogin` and
`PollDeviceLogin` to `auth.proto` and regenerated **only** `packages/tddy-web/src/gen/auth_pb.ts`;
the manifest lists two gen directories over `../tddy-service/proto`, so the second one is 197 lines
behind its proto and `scripts/generated-code.sh check` — a CI gate — fails on 2/9 and on every node
above it. It is reverted out of this commit rather than absorbed: the file belongs to 2/9's diff, and
fixing a parent's omission here would show in *this* PR as a change to a service it does not own. It
is fixed by amending 2/9 during the stack's closing cascade.

## Acceptance Criteria

- [ ] `ListAccounts` returns records grouped by provider with **no secret in any field**
- [ ] Empty, locked and errored render as three distinct, explained states
- [ ] `SetAccountLabel` leaves `account_id` stable, so 5/9's assignments survive a rename
- [ ] `RemoveAccount` removes exactly one record, behind a confirmation
- [ ] An invalid `session_token` is refused rather than served an empty list
- [ ] `/accounts` is reachable from the nav menu and by direct URL
- [ ] The screen is not capability-gated and adds no new reader of `capabilities`
- [ ] `tddy-credentials` gains no dependency on `tddy-service`

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [ ] M1–M9
- [ ] Package documentation for `tddy-accounts` and `tddy-web`
- [ ] `/wrap-context-docs` — this node claims **no** backlog entry and **no** code-issue record
