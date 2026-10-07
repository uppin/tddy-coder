# Screen-sharing service

`tddy-screen-sharing` serves `screen_sharing.ScreenSharingService` and spawns the VNC/RDP bridge
processes it streams from. It holds **no cryptography and no storage of its own**: a desktop's
password is a record in the signed-in user's credential store (`tddy-credentials`).

Product behaviour: [screen-sharing-sessions.md](../../../docs/ft/web/screen-sharing-sessions.md).

## Modules

| Module | Owns |
|---|---|
| `screen_sharing_records` | the `screen-sharing` provider's record shape (`record_for`, `target_from`), `TargetError`, and the `ScreenSharingTargetStore` port |
| `session_vault_target_store` | `SessionVaultTargetStore`, the production store over the daemon's `SessionVaults` |
| `screen_sharing_service` | `ScreenSharingServiceImpl`: the session-scoped target calls, the host-scoped calls, bridge spawning and termination |

## A target is a record

A second provider in the credential store costs a `ProviderId` and a mapping, and no
provider-specific storage, crypto or sync code.

| Target field | In the record |
|---|---|
| `id` | the `AccountId` — two desktops are two accounts of the `screen-sharing` provider |
| `label` | `label` |
| `host`, `port`, `protocol`, `username` | `metadata` under `host`, `port`, `protocol`, `username` |
| password | `secret` |

Everything is inside the sealed record, so tampering with `host` in the stored file fails the open.
The protocol is stored as the proto enum's **name** (`VNC`, `RDP`) because a record outlives the
build that wrote it. `target_from` returns `TargetError::Malformed` for a record missing a required
key or holding an unparseable port or protocol — it never defaults a host.

Targets are **per user**: the store is per user, so a target added in one session is listed in the
next. Because a screen-sharing record is an ordinary record, credential propagation
(`tddy-credential-sync`) carries it between daemons with no screen-sharing code in that path, and
the Accounts screen lists the provider like any other.

## No passphrase exists

The session is the key. `ScreenSharingService` has no unlock RPC, and no message in its schema has a
passphrase field; `tddy-service`'s `screen_sharing_carries_no_passphrase` test asserts that over the
schema, so a reintroduction fails a test. Existing `.screen-sharing.yaml` files are never read;
targets are added again once.

## The store port

`ScreenSharingTargetStore` is expressed over `session_token` and targets rather than over a
`SessionVault`, because resolving a token to the vault it opens is the daemon's job:

- `list`, `add` (the store mints the id), `remove`, and `password_for`.
- `password_for` is separate from `list` because a listing crosses an RPC boundary and a password
  must not.

`TargetError` has four outcomes: `NoSuchSession`, `Locked`, `Malformed`, `Unavailable`.

### Locked is a state, not an empty list

A vault that exists and is not open on this daemon is `Locked`. `ListTargets` answers it as
`ListTargetsResponse.vault_locked = true` with no targets, so a screen can tell open-and-empty from
locked. Every other call turns `Locked` into `FAILED_PRECONDITION`. `SessionVaultTargetStore`
never reports a closed vault as an empty listing, and server-side I/O detail never reaches the
client — it is logged and replaced by a fixed sentence.

## Wiring

`ScreenSharingServiceImpl::with_credential_vaults(Option<Arc<SessionVaults>>)` builds a
`SessionVaultTargetStore` over the daemon's vaults, using the service's own session-token
resolver as the subject resolver. With `None` (no `auth_storage`) no store is wired and the target
calls return `FAILED_PRECONDITION` rather than answering "no targets". `with_target_store` accepts
any `ScreenSharingTargetStore`, which is how the acceptance suite substitutes storage.

## Unchanged by the store

`StartStream`, `StopStream`, bridge identity (`screenshare-<session>-<target>`), track naming,
dimensions, the host-scoped calls (host desktops are prompted for, never stored), and VNC input.
`start_stream` reads a target and its password from the store and hands the password to the bridge on
its stdin.

## Tests

| Suite | Holds still |
|---|---|
| `tests/screen_sharing_records_unit.rs` | the record mapping, malformed records, the AEAD tamper |
| `tests/screen_sharing_service_acceptance.rs` | the service, over an in-memory store that uses the real mapping |
| `tests/session_vault_target_store_acceptance.rs` | the store, over a real sealed vault: cross-session listing, locked, cleartext leakage |
| `tests/screen_sharing_record_propagation.rs` | reconciliation of a screen-sharing record through the sync engine |
| `tddy-service/tests/screen_sharing_carries_no_passphrase.rs` | the schema declares no unlock RPC and no passphrase field |
