# 2026-11-16 — Connect a Coder deployment as an account

**Type:** Feature · `#remote-shell` 2/7, PR [#916](https://github.com/uppin/tddy-coder/pull/916), base `feature/remote-shell/shell-port`, wave 1 of 4

A signed-in person connects a Coder deployment on the Accounts screen by entering its URL and
a session token. The daemon checks the pair against the deployment, seals it in the person's
credential vault as a `coder` account, and lists it beside their GitHub and provider accounts.

## What changed

- **tddy-service**: `accounts.proto` gains `ConnectCoderAccount` with its request and response
  messages; the TypeScript client is regenerated in **tddy-client-web** and
  **tddy-rust-typescript-tests**.
- **tddy-accounts**: `src/coder_account.rs` holds the record codec (`CoderAccount`,
  `CoderDeploymentUrl`), the `CoderDeploymentProbe` port, and `CoderAccounts::{connect,
  credentials_for}` — the read path later nodes use to get a connected deployment's URL and
  token from an open vault. `service.rs` gains one delegating RPC arm. Crate docs:
  [`packages/tddy-accounts/docs/coder-accounts.md`](../../packages/tddy-accounts/docs/coder-accounts.md).
- **tddy-daemon**: `HttpCoderDeploymentProbe` answers the port over the daemon's `reqwest`
  client (`GET /api/v2/users/me`, token in the `Coder-Session-Token` header), wired in
  `account_linking::accounts_service_over`.
- **tddy-workspace-web**: the Coder group and `ConnectCoderAccountForm` on the Accounts screen;
  the two connect controls live in `ConnectAccountControls.tsx`.

## Verification

Scoped `./test -p tddy-accounts -p tddy-daemon -p tddy-service`: 69 test binaries, 0 failed
(2026-11-16). Scoped clippy with `-D warnings` and `cargo fmt` clean. Cypress component spec
`ConnectCoderAccountAcceptance`: 3 passed, 0 failed.

## File length

- `AccountsScreen.tsx` 463 → 516 production lines in this PR, decomposed in this PR to 434:
  `ConnectCoder` and `ConnectApiKey` moved to `ConnectAccountControls.tsx` (93 lines).
- `packages/tddy-accounts/src/service.rs` 772 → 790 production lines. Not split here: this PR's
  boundaries forbid it, and the split is deferred by
  `docs/dev/todo/2026-10-21-tddy-accounts-service-rs-over-file-budget.md` to a follow-up branch
  after the `#remote-shell` stack lands. The final measurement is recorded in
  `packages/tddy-accounts/docs/code-issues/oversized-file-service.md`.
- `packages/tddy-rust-typescript-tests/gen/accounts_pb.ts` 933 → 1006 lines: generated output,
  alert only.

## Code issues

- `packages/tddy-accounts/docs/code-issues/oversized-file-service.md` — re-measured
  772 → 790 production lines (worse: the one delegating RPC arm, its field and two imports).
  The record stays open with the new measurement; the split belongs to the follow-up above.
- `packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md` and
  `packages/tddy-workspace-web/docs/code-issues/oversized-file-use-model-registry-fan-out.md`
  name code this PR did not touch; both are left as they stand.
- This PR adds no code issue: the new Rust module is 402 production lines, every function in
  the changed Rust files is within 40 lines, and the web split above keeps the screen under
  budget.

## Backlog

No `docs/dev/todo/` entry is resolved here. The `service.rs` entry above gains this PR's
measurement and stays open. The GitHub add-account entry
`2026-10-07-keyring-link-github-add-account-ungated.md` stays open: the Coder form does not
repeat it (it is shown only where the accounts service is registered, and a refusal is
displayed), and fixing the GitHub control is its own change.
