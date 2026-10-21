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
| `screen_sharing_service` | `ScreenSharingServiceImpl`: the struct and builders, the session-scoped target calls, and the trait impl, whose host-scoped methods delegate to `host_targets` |
| `screen_sharing_service::bridge_lifecycle` | bridge spawning, the wait for its start report, and termination; the desktop-password prompt and the target-store lookup, as `impl ScreenSharingServiceImpl` members |
| `bridge_readiness` | `await_bridge_readiness`: reading a bridge's start report from its stdout within a bound, and mapping it to `BridgeStarted { width, height }` or a `Status`; `DEFAULT_BRIDGE_READY_TIMEOUT` (30 s) |
| `screen_sharing_service::host_targets` | the bodies of the host-scoped RPCs (`list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream`) as `*_impl` members |
| `bridge_config` | `BridgeSpawnConfig` and `PreparedBridge`, how a bridge's spawn configuration is built (`prepare_bridge`, `build_bridge_spawn_config`), and the bridge key, identity, track-name and LiveKit-room naming |

## A start waits for its bridge

`spawn_bridge` pipes the bridge's stdin and stdout, writes the `BridgeSpawnConfig` JSON to stdin and
closes it, records the pid, then waits for the bridge's start report
([`tddy_daemon_extensions::bridge_readiness`](../../tddy-daemon-extensions/docs/bridge-readiness.md))
through `await_bridge_readiness`:

| Report | Start returns |
|---|---|
| `Ready { width, height }` | `StartStreamResponse` with that size |
| `Failed { kind: Authentication }` | `Unauthenticated`, carrying the desktop's reason |
| `Failed { kind: Connect \| LiveKit }` | `Unavailable`, carrying the bridge's reason |
| stdout closes first | `Unavailable` |
| nothing within the timeout | `DeadlineExceeded` |

On every failure the child is killed and its pid forgotten (only while the key still names that pid)
before the call returns. The pid is recorded **before** the wait, so a stop that arrives while a
bridge is still starting reaches it. On success a background task reaps the child when it exits.
The timeout is `DEFAULT_BRIDGE_READY_TIMEOUT` unless `with_bridge_ready_timeout` sets another.

Both scopes report the outcome. `start_host_stream` and the session-scoped `try_spawn_bridge` return
the bridge's size or its failure, and a session target whose bridge cannot be prepared
(it names no protocol, the daemon has no LiveKit configuration, or no room token can be minted) fails `FAILED_PRECONDITION`. `BridgeSpawnConfig.width/height` are `0`: no
size is hinted to a bridge, which takes it from the desktop. A session start with no daemon config,
or for a target that is not in the store, spawns nothing and answers with a `0×0` size.

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
the host-scoped calls (host desktops are prompted for, never stored), and VNC input.
`start_stream` reads a target and its password from the store and hands the password to the bridge on
its stdin.

## Tests

| Suite | Holds still |
|---|---|
| `tests/screen_sharing_records_unit.rs` | the record mapping, malformed records, the AEAD tamper |
| `tests/screen_sharing_service_acceptance.rs` | the service, over an in-memory store that uses the real mapping |
| `tests/bridge_readiness_acceptance.rs` | a start against a `/bin/sh` stand-in bridge scripted per test (`tests/common`): refused sign-in, reported size on both scopes, early exit, silence past the timeout, and that a failed bridge is stopped |
| `tests/session_vault_target_store_acceptance.rs` | the store, over a real sealed vault: cross-session listing, locked, cleartext leakage |
| `tests/screen_sharing_record_propagation.rs` | reconciliation of a screen-sharing record through the sync engine |
| `tddy-service/tests/screen_sharing_carries_no_passphrase.rs` | the schema declares no unlock RPC and no passphrase field |
