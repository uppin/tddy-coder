# Account linking

How `tddy-accounts` adds a second account to a person's credential vault without becoming it.
Product view: [account linking](../../../docs/ft/daemon/account-linking.md).

## The boundary: a link never mints a session

Logging in returns a `session_token`; completing a login *is* becoming that person. Linking runs the
provider's authorization dance and stores a record, and nothing more.

- `accounts.proto`'s `PollLinkAccountResponse` has no token field, and
  `tests/` assert it over the serialised response.
- `AccountLinker` has no method that produces a session and no access to the store.
- The daemon adapter drives a `GitHubOAuthProvider` (`tddy-github`), which has no notion of a session.
  Login's session-minting step is not reachable from `tddy-daemon/src/account_linking.rs`.

## Ports (`src/linking.rs`)

| Port | Responsibility |
|---|---|
| `AccountLinker` | `begin(provider)` yields a `LinkChallenge`; `poll(link_id)` yields a `LinkProgress`. The provider's dance only. |
| `LinkedAccountStore` | `held` (what the vault has at a provider), `put` (write a record), `session_account` (which account the session was established with). |

`AccountStore` (read and curate) is deliberately separate: writing a new credential is a different
capability. `AccountsServiceImpl::with_linking(linker, store)` wires both; without it the two link
RPCs answer `FAILED_PRECONDITION`. Nothing is substituted for the missing ports.

## Dedup on the provider's subject id

`record_for_link` matches held records on `META_SUBJECT_ID` (the GitHub user id), never on the login
name, which can be renamed and re-registered by someone else. A match is a **re-link**: `account_id`
and `label` are kept (so project assignments still resolve), the secret and `updated_at` are replaced,
`version` is bumped. No match creates `github-<subject id>`.

Login records the id under `github_id` and names the account by login. The daemon store presents that
key as `subject_id` in `held`, so linking the session's own GitHub user updates its record in place.

## Session account

The account the session was established with is the one the vault's key derives from.
`RemoveAccount` refuses it (`removal_allowed`, `FAILED_PRECONDITION` with the reason), and
`ListAccountsResponse.session_account` marks it. On the daemon it is `github` / the session's login.

## Outcomes

`LINK_PENDING`, `LINK_LINKED`, `LINK_DENIED`, `LINK_EXPIRED`, `LINK_VAULT_LOCKED`. A provider refusal
and a vault that cannot take the result are never conflated. A vault that is sealed or does not exist
yet is `LINK_VAULT_LOCKED` (`LinkError::Locked`). `link_id` is the daemon's handle; the provider's
device code never leaves it.

## Attempt lifetime

The service keeps each pending attempt with the deadline `expires_in_seconds` gave at begin. Begin and
poll drop attempts past their deadline; a poll of one that has expired answers `LINK_EXPIRED` and
removes it, so an abandoned attempt does not outlive its code. A finished attempt (terminal poll) is
removed at once. The daemon linker keeps the same bound on the device codes it holds.

The web page waits `interval_seconds` before the first poll, and after each pending answer waits the
interval the daemon last returned.

## Daemon wiring (`tddy-daemon`)

- `account_linking::GitHubAccountLinker` over `tddy_daemon_auth::github_account_linking_provider`, which
  builds a provider from the login `client_id` (same scopes as login, device flow only, no secret).
  It returns `None` for a stub, whose tokens are synthetic; the link RPCs then stay unwired.
- `account_linking::VaultLinkedAccountStore` over the open `SessionVaults`.
- `runtime::build` calls `with_linking` where it registers `AccountsService`.
- The ports are synchronous and the provider async, so the adapter blocks with
  `tokio::task::block_in_place`; it needs the daemon's multi-thread runtime.

Using a linked account for git and API operations is not part of linking.

## Poll pacing

The provider's interval is enforced server-side, not left to the client. Each attempt records when
the provider may next be asked; a poll arriving earlier is answered `LINK_PENDING` (carrying the
interval) without calling the provider. The first poll after a begin is not held back. "Now" comes
from `AccountsServiceImpl::with_clock`, so tests cross an interval or a window without sleeping.
Attempt deadlines are dated and forgotten by `deadline.rs`, shared with the daemon's linker.
