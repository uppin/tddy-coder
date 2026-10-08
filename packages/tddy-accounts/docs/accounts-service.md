# Accounts service

`tddy-accounts` serves `accounts.AccountsService` — what a person can **see and curate** in the
credential vault `tddy-credentials` seals ([credential store](../../tddy-credentials/docs/credential-store.md)).
It is the surface the `/accounts` screen in `tddy-web` reads
([accounts screen](../../tddy-web/docs/accounts-screen.md)); the product view is
[docs/ft/web/accounts-screen.md](../../../docs/ft/web/accounts-screen.md).

Three methods that read and curate, plus two that link ([Linking](#linking)). Linking is the only way
a credential enters the vault through this service, and it never produces a session.

| RPC | Does |
|---|---|
| `ListAccounts(session_token)` | every record in the caller's vault, grouped by provider |
| `SetAccountLabel(session_token, provider, account_id, label)` | renames one record; answers with the record as it now stands |
| `RemoveAccount(session_token, provider, account_id)` | forgets one record; answers with what remains, grouped as `ListAccounts` groups it |

The proto is `packages/tddy-service/proto/accounts.proto`, generated as an async trait plus an
`RpcService` server and no tonic adapter: a browser asks it over whatever wire the host connection
was opened on, and nothing addresses it over the daemon's local Unix socket.

## No secret on the wire

`AccountSummary { provider, account_id, label, subject, updated_at, has_secret, sync_status }` has
**no secret field**, and no method returns a credential. The rule is enforced by message shape
rather than by discipline: there is nothing to fill. `has_secret` is the one bit a screen needs to
tell a linked account from a stale row. `subject` is the record's `metadata["subject"]` — the
provider's own identifier (a GitHub login), shown beside the label.

`no_byte_of_a_listing_on_the_wire_is_a_secret` asserts this over the **encoded** response, not the
struct, so a field added later that carried one would fail it.

## Identity is stable, the label is not

`account_id` never changes; the label is the only mutable field. A rename therefore cannot break
anything keyed by `(provider, account_id)`, such as a project's assigned account.

## Five inputs, five answers

The service distinguishes every state a person must be told apart. None is collapsed into an empty
list, because that would present a recoverable state as a normal one and lead a person to re-link
accounts they already have.

| Store answer | `ListAccounts` | `SetAccountLabel` / `RemoveAccount` |
|---|---|---|
| `Ok(records)` (possibly empty) | `providers`, both flags false | the result |
| `AccountsError::Uninitialized` — no vault for this subject | `vault_uninitialized: true` | `FailedPrecondition`: "no credential vault exists … choosing a passphrase creates one" |
| `AccountsError::Locked` — a vault exists, not unlocked on this daemon | `vault_locked: true` | `FailedPrecondition`: "unlock it with your passphrase — nothing in it is lost" |
| `AccountsError::NoSuchSession` — the token names no session | `Unauthenticated` | `Unauthenticated` |
| `AccountsError::NotFound { provider, account }` | — | `NotFound`, naming the provider and the account (a rename of an account that is not linked) |
| `AccountsError::Unavailable(reason)` — I/O, corruption, anything else | `Internal` carrying `reason` | `Internal` carrying `reason` |

Removing a record that is not there is **not** an error: the caller wanted it gone, and it is.

## The `AccountStore` port

`AccountsServiceImpl<S: AccountStore>` reads the vault through a port (`src/store.rs`) expressed
over `tddy-credentials`' **data** types — `CredentialRecord`, `ProviderId`, `AccountId` — not over
`SessionVault`. A `SessionVault` can only be obtained by sealing or opening a real file under a
passphrase, so a test of this crate's grouping and mapping would otherwise be a test of that crate's
cryptography. The acceptance suite (`tests/accounts_service_acceptance.rs`) drives the service over
an in-memory store.

### `SessionVaultAccountStore` — the production store

`src/vault_store.rs` implements the port over the daemon's `SessionVaults`, one registry per daemon,
and a `SessionSubjectResolver` — `Arc<dyn Fn(&str) -> Option<String>>` — that maps a session token
to the subject whose vault it may open. The resolver is injected: verifying a session token is the
daemon's job, and doing it here would pull in the auth stack this crate is kept free of.

- `SessionVaults::state(subject)` decides first: `Locked` and `Uninitialized` refuse as such;
  `Open` is read through `SessionVaults::use_open`, which **counts as a use** — a person looking at
  their accounts keeps the vault open for another idle lifetime.
- A vault that closes between the two look-ups (idle eviction, the last sign-out) answers with
  whatever it is now.
- `VaultError::Io` names server-side paths, so the client gets the fixed, path-free sentence
  "the credential store could not be read or written on this daemon" and the daemon log
  (`target: "tddy_accounts"`) gets the full error with its subject. Every other `VaultError` is a
  fixed sentence written for the person and is passed on verbatim; `VaultError::Locked` keeps its
  meaning.

⚠ **Known limitation**: `set_label` reads the record and writes it back as two vault operations,
each serialised on its own, so a write to the same record landing between them is overwritten.
Closing it needs an atomic update on `SessionVault`; tracked in
`docs/dev/todo/2026-10-04-keyring-accounts-rename-lost-update.md`.

## Resolving a project's account

`resolve_account(assignments, provider, held) -> AccountResolution` answers which account a project
uses for a provider. `assignments` is the project row's whole set
([`ProjectData.accounts`](../../tddy-projects/docs/project-service.md#account-assignments)) and
`held` is what this host's vault holds for the session doing the work. Both are plain data, so it is
a pure function: reading the vault and gating the session happen before it.

| Answer | When |
|---|---|
| `Assigned(AccountId)` | exactly one account is assigned for the provider and this host's vault holds it |
| `NotAssigned` | no account is assigned for the provider |
| `UnknownOnThisHost(AccountId)` | an account is assigned, but this host's vault holds no such record |
| `Ambiguous(ProviderId)` | more than one account is assigned for the provider |

**`NotAssigned` resolves to nothing, and nothing is a valid answer.** There is no fallback: not to
the caller's own login, not to the only account in the vault, not to an unauthenticated request.
A project with no GitHub account assigned has no GitHub credentials, and an operation that needs
them says so rather than guessing, because a guess would act under the wrong identity. A vault
holding exactly one candidate does not change the answer.

`UnknownOnThisHost` is separate from `NotAssigned` because the person did choose: the choice is
intact and this host cannot see the account. `Ambiguous` is unreachable through
`SetProjectAccounts`, which refuses two accounts for one provider; it is an answer rather than a
panic because the registry file can be edited by hand, and reaching it is a defect to report.

The resolver lives here rather than in `tddy-projects` because answering `UnknownOnThisHost` needs
the vault. `tddy-projects` does not depend on `tddy-credentials`, so the credential store is not on
the dependency path of every project consumer.

## `SyncStatusSource` — `#keyring` 6/9's sync status, optional and additive

`AccountSummary.sync_status` reports where this account stands with the peers this deployment
propagates credentials to — the single worst status across every peer
(`tddy_credential_sync::AccountSyncSummary`, mapped onto the wire's `SyncStatus` enum), not a
per-peer breakdown. `SYNC_STATUS_UNSPECIFIED` is what every account reports when nothing is wired —
no sync engine running on this daemon at all, which is the common case for a daemon with no
`keyring.group_secret` configured.

`AccountsServiceImpl::with_sync_status(Arc<dyn SyncStatusSource>)` wires it in; `::new` alone leaves
it `None`, so every existing construction site (including `tddy-daemon`'s own) keeps reporting
`UNSPECIFIED` unchanged. The port (`src/sync_status.rs`) is expressed over
`tddy_credential_sync::AccountSyncSummary` rather than this crate re-deriving the aggregation —
duplicating that severity ordering here would be a second place it could drift from the one in
`tddy-credential-sync`. `tddy-daemon`'s `credential_sync` module is what actually implements the
port, reading a live `SyncEngine`'s journal; see `tddy-credential-sync`'s own
`docs/credential-sync.md` for the engine side.

## Linking

`BeginLinkAccount(session_token, provider)` and `PollLinkAccount(session_token, link_id)` add an
account without replacing the session, and `ListAccountsResponse.session_account` marks the account
the session was established with (which `RemoveAccount` refuses to remove). They are served only when
`AccountsServiceImpl::with_linking` was called; otherwise both answer `FAILED_PRECONDITION`. Detail:
[account-linking.md](./account-linking.md).

## Registration

`build_accounts_entry(service)` returns the `ServiceEntry` named `accounts.AccountsService`. The
daemon registers it only when `auth_storage` gives it credential vaults — see
[daemon-endpoint.md](../../tddy-daemon/docs/daemon-endpoint.md). Without them there is no vault to
show, and nothing stands in for one.

## Dependencies

`tddy-credentials`, `tddy-credential-sync`, `tddy-service`, `tddy-rpc` — **not** `tddy-daemon-auth`
and not LiveKit. The service lives in its own crate rather than in `tddy-credentials` so the store
stays free of proto and RPC, and the crates that depend on the store do not pay for them.
`tddy-credential-sync` is a lightweight addition — it depends on `tddy-credentials` alone, nothing
LiveKit or daemon-auth shaped — reached only for `AccountSyncSummary`, the type
`SyncStatusSource` is expressed over.

## Acting as an account

Which account a project acts as, for the token and the commit identity a session uses, is answered by
`acting_identity`, not by this service: [github-identity-resolution.md](github-identity-resolution.md).
