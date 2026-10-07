# 2026-10-07 — Add account is shown on daemons that cannot link

**Category:** Deferred scope from `#keyring` 8/9 `link-github` (PR #515)
**Source:** `AccountsScreen` (`packages/tddy-web/src/components/accounts/AccountsScreen.tsx`) and
`AccountsServiceImpl::require_linking` (`packages/tddy-accounts/src/service.rs`)

The Accounts screen offers **Add account** on every provider group, including on a daemon that wired
no linking — a daemon whose GitHub provider is a stub (`github_account_linking_provider` returns
`None`), or one without a usable `client_id`. There `BeginLinkAccount` answers `FAILED_PRECONDITION`,
and the person presses a control that cannot work and is shown the daemon's reason.

**Why deferred:** closing it needs a capability bit on `ListAccountsResponse` (a proto field saying
whether linking is wired), which is a wire change beyond `#keyring` 8/9's node.

## What stands today

- The screen shows Add account unconditionally; the daemon refuses with `FAILED_PRECONDITION` and the
  screen displays the reason. Nothing is substituted, and nothing is stored.
- **Attempt pacing:** the provider's poll interval is enforced server-side (an early poll is answered
  pending without asking the provider), but the **first poll after begin is not rate-limited
  server-side** — the page waits the interval, a client that does not can ask the provider at once.

## What would close it

1. A capability field on `ListAccountsResponse`, set from whether `with_linking` was called.
2. The screen reads it and hides (or disables, with the reason) Add account when it is false.
3. Optionally, hold the first poll back by the interval too, so pacing does not depend on the client.

Delete this entry once the control follows the daemon's capability.
