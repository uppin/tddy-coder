# PRD: Accounts — the screen and service over the credential store

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Stack**: `#keyring` 4/9 · branch `feature/keyring/accounts` · base `feature/keyring/store` (#510)

## Affected Features

- **Accounts screen** — `docs/ft/web/accounts-screen.md`, written by this change
- [App shell](../app-shell.md) — one navigation entry
- [Capability gating](../../../../packages/tddy-web/docs/capability-gating.md) — why this screen is **not** gated
- [Cross-daemon session authentication](../../daemon/session-auth.md)

## Summary

`#keyring` 3/9 gives a daemon a place to keep credentials. Nothing yet lets a person see what is in
it, name an entry, or remove one.

This node adds `accounts.proto`, an `AccountsService` that reads and curates the vault **without ever
returning a secret**, and an `/accounts` screen in `tddy-web` that lists a daemon's linked accounts
per provider.

It is the last node before the store becomes useful to the rest of the stack: 5/9 assigns account
identifiers to projects, 7/9 turns screen-sharing into a provider, and 8/9 links a second GitHub
account. All three need a surface where a person can see what exists.

## Background

### What exists

`tddy-web` has eight daemon-scoped screens behind `DaemonNavMenu` — Sessions, Worktrees, Tasks,
Projects, Models & Agents, Hosts, VMs, LiveKit — each a `*AppPage` component selected by a pure
predicate in `src/routing/appRoutes.ts` and dispatched from the ladder in `src/index.tsx:483-499`.
The pattern is uniform and small: a route constant, an `is*Path` predicate, a nav `Button` with a
`data-testid`, and an entry in the ladder.

Every daemon RPC carries a `session_token` as its first field (`project.proto` is the model), which
is what identifies the caller and — after 3/9 — what scopes the opened vault.

### What is missing, and why it is not a detail

After 3/9 a credential is a `(provider, account, secret)` record in an encrypted file. A person has
**no way to observe that file's contents**. That matters beyond convenience:

- 5/9 asks a person to assign an account to a project. An assignment UI that cannot show what is
  assignable is not usable.
- A vault that reports `Locked` — the deliberate no-fallback outcome when the login credential
  changed — has nowhere to say so. Without this screen, the only symptom a person sees is git
  operations failing later with no explanation.
- A stale record (an account revoked at the provider) can never be removed.

## Proposed Changes

### `accounts.proto` — a new service, no secrets on the wire

```proto
service AccountsService {
  rpc ListAccounts(ListAccountsRequest) returns (ListAccountsResponse);
  rpc SetAccountLabel(SetAccountLabelRequest) returns (SetAccountLabelResponse);
  rpc RemoveAccount(RemoveAccountRequest) returns (RemoveAccountResponse);
}

message AccountSummary {
  string provider = 1;      // "github", "cloudflare", …  open, not an enum
  string account_id = 2;    // daemon-minted, opaque, stable
  string label = 3;         // human-chosen; defaults to the provider's own handle
  string subject = 4;       // the provider's identifier for the account, for display
  int64  updated_at = 5;
  bool   has_secret = 6;    // whether a usable credential is present
}
```

**`AccountSummary` carries no secret field, and there is no `GetAccount` returning one.** 3/9 made
"no store API returns a secret to an RPC response path" a boundary; this node is where that boundary
would most plausibly be crossed, so it is enforced by the message shape rather than by discipline.
`has_secret` is the one bit a UI needs, and one bit is not a credential.

### Three list outcomes, never collapsed

`ListAccountsResponse` distinguishes states the UI must not render identically:

| Outcome | Meaning | What the screen shows |
|---|---|---|
| `accounts: []` | the vault opened and is empty | "No accounts linked yet", with what to do |
| `vault_locked` | the login credential changed; the KEK no longer unwraps | the lock, and that re-linking is the recovery |
| error | I/O, corruption | the error, verbatim |

⚠ **Collapsing `vault_locked` into an empty list would be a fallback** in the sense CLAUDE.md
forbids: it presents a recoverable, explainable failure as a normal empty state, and the person
re-links accounts they already have instead of understanding what happened. The three stay distinct
end to end — `VaultError::Locked` → a distinct response → a distinct rendering.

### `/accounts` screen

Follows the existing screen pattern exactly — `ACCOUNTS_ROUTE = "/accounts"`, `isAccountsPath`, an
`AccountsAppPage`, a `shell-menu-accounts` nav entry, and one rung in `index.tsx`'s ladder. Grouped
by provider, each row showing label, subject, last update and whether a credential is present.

Two actions: **rename** (the label is the only mutable field — `account_id` is stable precisely so a
rename cannot break 5/9's assignments) and **remove**, behind a confirmation naming what breaks.

**Linking a new account is deliberately not here.** Each provider's link flow is its own work:
GitHub's is 8/9, screen-sharing's is 7/9. This screen shows and curates what exists.

### Not capability-gated

`useHasCapability` exists for **media and presence** — things a wire either carries or does not.
Accounts is plain RPC over whatever wire the host connection was opened on, so it is not gated, and
this node adds **no fourth place** that reads a `capabilities` set. Stated because the screen is
daemon-scoped and gating it would look superficially consistent; the capability-gating doc is
explicit that the single predicate is a deliberate singularity.

## What's Staying the Same

- The vault format, key derivation and `SessionVault` — 3/9 owns all three; this node only calls them.
- Every other screen, route and nav entry.
- `useHasCapability` and `capabilityAvailability` — untouched.
- Project↔account assignment (5/9), cross-daemon propagation (6/9), screen-sharing as a provider
  (7/9), second GitHub account (8/9).

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-service` | New `proto/accounts.proto`; generated Rust and TS |
| `tddy-accounts` (**new**) | The service implementation over `SessionVault` |
| `tddy-daemon` | One `rpc_entries.push(...)` beside the others in `runtime.rs` |
| `tddy-web` | Route constant + predicate, `AccountsAppPage`, nav entry, one ladder rung |
| `tddy-credentials` | **Unchanged** — this node consumes 3/9's API and adds none |

**Why a new crate rather than putting the service in `tddy-credentials`**: 3/9 deliberately kept the
store free of `tddy-service` and of the auth crate, so 6/9 and 7/9 can depend on it cheaply. Adding
proto and RPC to it would push that cost onto every dependent —
`heavy-dependency-livekit-peer-forwarding` is this repo's measured example of what one misplaced
dependency costs: 14 dependents, 6 of them paying for something they never use.

## Implementation Plan

1. `accounts.proto` — the service and its three messages.
2. `tddy-accounts` — `ListAccounts` over `SessionVault`, with the three outcomes distinct.
3. `SetAccountLabel` and `RemoveAccount`.
4. Register the entry in `runtime.rs`.
5. `tddy-web`: route constant, predicate, and the `appRoutes` unit tests beside the existing ones.
6. `AccountsAppPage` + the accounts list, with the three states rendered distinctly.
7. Nav entry and the `index.tsx` rung.
8. Cypress component tests with `mountWithRpc` + `anInMemoryRpcBackend`; a Storybook story.

## Acceptance Criteria

- [ ] `ListAccounts` returns the vault's records grouped by provider, **with no secret in any field**
- [ ] An empty vault, a locked vault and an error render as three distinct, explained states
- [ ] `SetAccountLabel` changes only the label; `account_id` is unchanged and assignments survive
- [ ] `RemoveAccount` removes exactly one record, behind a confirmation
- [ ] An unauthenticated or invalid `session_token` is refused, not served an empty list
- [ ] `/accounts` is reachable from the nav menu and by direct URL
- [ ] The screen is **not** capability-gated and adds no new reader of `capabilities`
- [ ] Cypress component tests cover the three list states and both actions
- [ ] No new `tddy-service` dependency is added to `tddy-credentials`

## References

- **Accounts screen** — `docs/ft/web/accounts-screen.md`, written by this change
- [Projects screen — multi-host](../projects-screen-multi-host.md) — the screen pattern followed
- [Capability gating](../../../../packages/tddy-web/docs/capability-gating.md)
- [`heavy-dependency-livekit-peer-forwarding`](../../../../packages/tddy-daemon-kernel/docs/code-issues/heavy-dependency-livekit-peer-forwarding.md)
