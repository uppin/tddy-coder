# PRD: Linking a second GitHub account

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Stack**: `#keyring` 8/9 · branch `feature/keyring/link-github` · base `feature/keyring/screen-share` (#514)

## Affected Features

- [Cross-daemon session authentication](../session-auth.md)
- [LiveKit and auth services](../auth-livekit-services.md)
- **Accounts screen** — `docs/ft/web/accounts-screen.md`, written by `#keyring` 4/9
- **Account linking** — `docs/ft/daemon/account-linking.md`, written by this change

## Summary

Lets a person add a **second GitHub account** to the credential store **without replacing the one
they are signed in with**.

Today the only way a GitHub credential enters the daemon is by logging in, and logging in *is* the
session. The developer's requirement — *"we should also have multiple github accounts per session"* —
needs those two to come apart: an account is a record, and exactly one of the records happens to be
the one the session was established with.

## Background

### Why linking cannot reuse the login flow

`ExchangeCode` (and `#keyring` 2/9's device-flow equivalent) returns a `session_token`, a
`refresh_token` and a `GitHubUser`. Completing it **establishes who you are**. Running it again to
add a colleague's account, or a personal account beside a work one, would sign the person out of the
first and in as the second — which is the behaviour the requirement rules out.

So linking is a distinct flow with a distinct terminal effect:

| | Login (2/9) | Link (this node) |
|---|---|---|
| Requires an existing session | no | **yes** — the vault must be open |
| Produces a session token | yes | **no** |
| Changes who the caller is | yes | **no** |
| Result | a session, and a record | **a record, and nothing else** |

### What the session account still is

`#keyring` 3/9 derives the vault's KEK from the **login** account's access token. That account is not
special in the store — it is a record like any other — but it *is* special in one respect: without
it, nothing decrypts. This node is where that asymmetry becomes visible to a person, and it has one
consequence worth stating before the design: **the session account cannot be removed while it is the
session account.**

## Proposed Changes

### Two RPCs on `AccountsService`

```proto
rpc BeginLinkAccount(BeginLinkAccountRequest) returns (BeginLinkAccountResponse);
rpc PollLinkAccount(PollLinkAccountRequest) returns (PollLinkAccountResponse);

message BeginLinkAccountRequest {
  string session_token = 1;
  string provider      = 2;   // "github" today; open, like every other provider string
}
message BeginLinkAccountResponse {
  string link_id          = 1;   // opaque, scoped to this session
  string user_code        = 2;   // shown to the person
  string verification_uri = 3;   // opened for the person
  int32  interval_seconds = 4;   // GitHub's own polling floor, passed through
  int64  expires_at       = 5;
}

message PollLinkAccountRequest {
  string session_token = 1;
  string link_id       = 2;
}
message PollLinkAccountResponse {
  LinkState      state   = 1;
  AccountSummary account = 2;   // set only when state = LINKED
}

enum LinkState {
  LINK_PENDING   = 0;
  LINK_LINKED    = 1;
  LINK_DENIED    = 2;
  LINK_EXPIRED   = 3;
  LINK_VAULT_LOCKED = 4;
}
```

The shape is 2/9's device flow with the session-minting half removed. `AccountSummary` is 4/9's, so a
linked account appears on the Accounts screen with no new rendering.

`LINK_VAULT_LOCKED` is a state rather than an error for the same reason 4/9 keeps `vault_locked`
distinct from an empty list: a person whose vault is locked has not failed to link an account, and
telling them the link was denied would send them to GitHub to fix something GitHub did not do.

### Linking is idempotent on the GitHub user id

An account already in the store, linked again, updates the existing record's secret and keeps its
`account_id`. It does not mint a second one.

This matters because `account_id` is what `#keyring` 5/9's project assignments point at. Minting a
fresh id on a re-link would silently detach every project that referenced the old one — a person
re-authorises an account whose token expired and finds their projects unassigned, with nothing
pointing at why.

Deduplication is on the GitHub **user id**, not the login name: a login name can be changed and
re-registered by someone else.

### Removing an account, and the one that cannot be removed

`RemoveAccount` (4/9) works for any linked account. For the **session's own** account it is refused,
with a reason that says why: the vault's key derives from it, so removing it makes every other record
unreadable. Signing out is the operation that ends a session; removing the account the session rests
on is not the same thing wearing a different name.

⚠ Refusing is a deliberate choice over the alternative of **removing it and locking the vault**,
which is technically coherent and leaves a person with a store they cannot open and no obvious route
back. Recorded so a reviewer can overrule it.

### Scopes

A linked account requests the same OAuth App scopes as login. A second account exists to *act* — 9/9
resolves it for git and API operations — so an account linked with read-only scopes would fail at
push time, long after the person would connect the two events.

### Where it appears

The Accounts screen (4/9) gains an **Add account** control that starts the flow, shows the user code
and the verification URI, and polls. Nothing else on the screen changes: a linked account is a row
like the session's own, distinguished by a marker on the one the session uses.

## What's Staying the Same

- **Login.** `ExchangeCode`, the device flow, `RefreshSession` and `Logout` behave exactly as before.
  This node adds a second way for a credential to arrive, not a second way to sign in.
- **Key derivation** (3/9) — still from the login account's token. A linked account's token derives
  nothing.
- **`AccountSummary`** (4/9) — no new field, and still **no secret**.
- **Assignments** (5/9) and **propagation** (6/9) — a linked account is assignable and syncs, because
  it is a record.

## Impact Analysis

| Package | Impact |
|---|---|
| `tddy-accounts` | `BeginLinkAccount` / `PollLinkAccount`, dedup on GitHub user id, the removal refusal |
| `tddy-service` | `accounts.proto` gains two RPCs, their messages and `LinkState` |
| `tddy-daemon-auth` | The device flow is reused **without** the session-minting half |
| `tddy-web` | **Add account** on the Accounts screen; the session account marked |
| `tddy-credentials` | **Unchanged** |

## Implementation Plan

1. Split 2/9's device flow so its token-exchange half is reusable without minting a session.
2. `BeginLinkAccount` / `PollLinkAccount`, with per-session link state.
3. Dedup on GitHub user id; re-link updates in place.
4. Refuse removal of the session's own account, with the reason.
5. The Accounts-screen control and the session-account marker.
6. Acceptance: two accounts coexist, the session is unchanged, re-linking preserves assignments.

## Acceptance Criteria

- [ ] Linking a second account leaves `session_token` and the signed-in identity **unchanged**
- [ ] Both accounts appear on the Accounts screen; the session's own is marked
- [ ] Re-linking an existing account keeps its `account_id` — **project assignments survive**
- [ ] Dedup is on the GitHub user id, not the login name
- [ ] Removing the session's own account is **refused**, with the reason
- [ ] Removing any other linked account succeeds
- [ ] A locked vault yields `LINK_VAULT_LOCKED`, never `LINK_DENIED`
- [ ] A denied or expired device flow is reported as itself
- [ ] `AccountSummary` still carries no secret
- [ ] Login, refresh and logout behave exactly as before

## References

- [Cross-daemon session authentication](../session-auth.md)
- [LiveKit and auth services](../auth-livekit-services.md)
- `packages/tddy-service/proto/auth.proto` — the login flow this node deliberately does not reuse
