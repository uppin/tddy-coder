# 2026-10-07 — Linking a second GitHub account

**Type:** Feature · `#keyring` 8/9, PR [#515](https://github.com/uppin/tddy-coder/pull/515)
Cross-package entry: [`docs/dev/changesets/2026-10-07-keyring-link-github.md`](../../../../docs/dev/changesets/2026-10-07-keyring-link-github.md)

`AccountsService` gains `BeginLinkAccount` / `PollLinkAccount`, which add an account to the caller's
vault without producing a session token. `AccountsServiceImpl::with_linking(linker, store)` wires the
`AccountLinker` and `LinkedAccountStore` ports; without it both RPCs answer `FAILED_PRECONDITION`.

- Dedup on the provider's subject id (`record_for_link`); a re-link keeps `account_id` and `label`
  and replaces the secret.
- `RemoveAccount` refuses the account the session was established with (`removal_allowed`);
  `ListAccountsResponse.session_account` marks it. `AccountSummary` gained no field and still carries
  no secret.
- Attempts are dated and forgotten on their deadline (`deadline.rs`, `attempts.rs`); the provider's
  poll interval is enforced server-side, the first poll after begin is not held back.

Detail: [account-linking.md](../account-linking.md), [accounts-service.md](../accounts-service.md#linking).
