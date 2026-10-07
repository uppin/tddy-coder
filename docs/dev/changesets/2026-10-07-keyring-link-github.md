# 2026-10-07 — Linking a second GitHub account

**Type:** Feature · `#keyring` 8/9, PR [#515](https://github.com/uppin/tddy-coder/pull/515)

Adds a second GitHub account to the credential vault without replacing the session. Linking is a
distinct flow from login: it needs an open vault, produces a record and no session token, and never
changes who the caller is.

## What changed, by package

- **`tddy-service`**: `accounts.proto` gains `BeginLinkAccount`, `PollLinkAccount`, their messages,
  `LinkState` and `ListAccountsResponse.session_account` (`SessionAccount`). Additive; nothing in
  `auth.proto` changes. A schema-level tripwire (`tests/linking_mints_no_session.rs`) asserts no link
  response carries a token.
- **`tddy-accounts`**: the `AccountLinker` and `LinkedAccountStore` ports, dedup on the provider's
  subject id, re-link preserving `account_id`, the `RemoveAccount` refusal for the session account,
  attempt lifetime and server-side poll pacing. Detail:
  [`account-linking.md`](../../packages/tddy-accounts/docs/account-linking.md).
- **`tddy-daemon-auth`**: `github_account_linking_provider`, a device-flow-only provider built from the
  login `client_id`; `None` for a stub. Login, refresh and logout are untouched (`auth.rs` only widened
  two items to `pub(crate)`).
- **`tddy-daemon`**: `account_linking.rs` — `GitHubAccountLinker`, `VaultLinkedAccountStore` and
  `accounts_service_over`, which `runtime::build` calls where it registers `AccountsService`.
- **`tddy-web`**: **Add account** on the Accounts screen, the attempt states, the session-account
  marker; the page waits the daemon's interval before its first poll.

## Code-issue measurements (this PR claims none)

- `oversized-file-runtime` (`runtime.rs`): 1,777 → 1,776 production lines, −1. Still over budget.
- `complexity-runtime-build` (`build`): 1,006 → 1,005 lines by brace matching, −1; no branch, exit or
  nesting added.
- `oversized-file-auth` (`auth.rs`): 622 → 622, unchanged; the new provider lives in
  `account_linking_provider.rs`.

All three records stay open, each with a measurement row for this PR.

## Deferred

The Accounts screen offers Add account on every provider group even where the daemon wired no
linking; closing it needs a capability bit on `ListAccountsResponse`, a wire change beyond this node.
Tracked in the backlog as `2026-10-07-keyring-link-github-add-account-ungated`.

## Verification

Scoped to the touched packages (`tddy-accounts`, `tddy-service`, the `tddy-daemon` linking tests) and
the single Cypress spec `AccountLinkingAcceptance`. Not exercised end to end: `runtime::build` and
GitHub's real device flow. Whole-workspace health is CI's.
