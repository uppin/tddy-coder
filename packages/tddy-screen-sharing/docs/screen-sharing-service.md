# Screen-sharing service

`tddy-screen-sharing` serves `screen_sharing.ScreenSharingService` and spawns the VNC/RDP bridge
processes it streams from. It holds **no cryptography and no storage of its own**: a session
desktop's password, and a host desktop credential an operator asked to be remembered, are records in
that user's credential store (`tddy-credentials`).

Product behaviour: [screen-sharing-sessions.md](../../../docs/ft/web/screen-sharing-sessions.md).

## Modules

| Module | Owns |
|---|---|
| `screen_sharing_records` | the `screen-sharing` provider's record shape (`record_for`, `target_from`), `TargetError`, and the `ScreenSharingTargetStore` port |
| `session_vault_target_store` | `SessionVaultTargetStore`, the production store over the daemon's `SessionVaults` |
| `screen_sharing_service` | `ScreenSharingServiceImpl`: the struct and builders, the session-scoped target calls, and the trait impl, whose host-scoped methods delegate to `host_targets` and whose `start_stream` delegates to `session_stream` (the `#[async_trait]` bodies sit in inherent `*_impl` methods, which the restructure engine can extract from) |
| `screen_sharing_service::bridge_lifecycle` | bridge spawning, the wait for its start report, and termination; `desktop_credential_for` (a remembered credential, or the `DESKTOP_CREDENTIALS` prompt) with `SignIn` / `Remembering`, and the target-store lookup, as `impl ScreenSharingServiceImpl` members |
| `host_desktop_sign_in` | `inspect_host_desktop` (`InspectHostDesktop`), `decrypt_desktop_credential_answer`, `remembered_account`, and `RememberedDesktopCredentials` (`usable`, `can_remember`, `remember`, `mark_refused`) |
| `bridge_readiness` | `await_bridge_readiness`: reading a bridge's start report from its stdout within a bound, and mapping it to `BridgeStarted { width, height }` or a `Status`; `DEFAULT_BRIDGE_READY_TIMEOUT` (30 s) |
| `screen_sharing_service::host_targets` | the bodies of the host-scoped RPCs (`list_host_targets`, `add_host_target`, `start_host_stream`, `stop_host_stream`) as `*_impl` members; `start_host_stream_impl` looks the target up with `find_host_target`, prepares the bridge and the sign-in, and hands them to `start_bridge_for_desktop` (the X11 `x11vnc` start, then the signed-in spawn) |
| `screen_sharing_service::session_stream` | the body of the session-scoped `start_stream` as `start_stream_impl`, and `start_session_bridge`: the target lookup → password → `BridgeTarget` → spawn ladder, run only when the daemon config is present |
| `x11vnc` | `X11vncServers`: one `x11vnc` per open X11 channel, keyed like its bridge; start (stopping the key's previous one first), wait until it accepts, stop; `x11vnc_arguments`, `X11VNC_HOST`, `DEFAULT_X11VNC_ACCEPT_TIMEOUT` (10 s) |
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

## An X11 channel runs x11vnc around its bridge

A host target may name an X11 display (`HostDesktopTarget.display`, from
`AddHostTargetRequest.x11_display`, returned on `ScreenSharingTarget.x11_display`) instead of a
listening `host:port`. `start_host_stream` for one does the following:

1. Terminates any bridge still under the target's key, **before** touching x11vnc. A reopen that
   stopped the old x11vnc first would make the old bridge exit while its key still named it, and
   that bridge's reaper would then stop the x11vnc being started.
2. `X11vncServers::start`: stops the key's previous x11vnc, spawns
   `x11vnc -display :N -localhost -rfbport <free> -nopw -forever` (binary from
   `resolve_x11vnc_binary_path`), and polls `127.0.0.1:<port>` until it accepts. If it exits
   first, the start fails with `Unavailable`. Past `DEFAULT_X11VNC_ACCEPT_TIMEOUT` (or
   `with_x11vnc_accept_timeout`) it fails with `DeadlineExceeded`, and a spawn error is `Internal`.
   On any failure the x11vnc is killed before the call returns, and no bridge is started.
3. Points the bridge's spawn config at `X11VNC_HOST:<port>` and spawns it (`spawn_bridge`,
   unchanged). If the bridge fails, its x11vnc is stopped.

The x11vnc is stopped by `StopHostStream`, and by `reap_bridge` when a bridge exits on its own while
its key still names it. A bridge that was stopped or replaced leaves the x11vnc alone, because its
stopper owns that. A background task reaps every x11vnc child.

## A host desktop's sign-in

**Inspect.** `inspect_host_desktop` finds the host target, and for an X11 display answers `[NONE]`
without running anything: the daemon's own `x11vnc` takes no password. Otherwise it resolves the
protocol's bridge binary (`Protocol::Unspecified` is `FAILED_PRECONDITION`). It runs that binary
with `--inspect` (`kill_on_drop`), writes an `InspectTarget { host, port }` JSON to its stdin and
reads the first line of its stdout as an `InspectReport`
([`tddy_daemon_extensions::desktop_inspect`](../../tddy-daemon-extensions/docs/desktop-inspect.md)).
It answers `InspectHostDesktopResponse`: `offered`, the methods tddy can sign in with in the order
advertised; `unsupported_vnc_types`, the VNC security types it cannot, by number; and `remembered`,
whether a usable remembered credential exists. A bridge that exits non-zero, prints nothing or
prints something unreadable fails it with `Unavailable` and the bridge's stderr. A bridge that runs
past 35 s (`INSPECT_TIMEOUT`) fails it with `DeadlineExceeded`.

**Credential.** `start_host_stream` settles the target, room and bridge first, then
`desktop_credential_for` returns a `SignIn`:

| Source | `Remembering` |
|---|---|
| a usable remembered credential (`usable`: the vault is open, a record exists, and it is not marked `refused`) | `AlreadyRemembered` |
| the operator's answer to a `DESKTOP_CREDENTIALS` prompt, without `remember` | `Nothing` |
| the same, with `remember` — after `can_remember` confirmed a vault is wired and open, or the start is refused `FAILED_PRECONDITION` before anything is spawned | `AfterStart` |

The answer is RSA-OAEP ciphertext of a protobuf `DesktopCredentialAnswer`.
`decrypt_desktop_credential_answer` maps a decrypt failure to `PERMISSION_DENIED`, and an undecodable
plaintext to `INVALID_ARGUMENT` without quoting it. `PreparedBridge::for_credentials` copies only
`username`, `domain` and `password` into the `BridgeSpawnConfig`.

**Settling the vault** (`start_signed_in_bridge`). A bridge refused with `Unauthenticated` marks an
`AlreadyRemembered` credential refused (`mark_refused`: metadata `refused`, the version bumped). An
`AfterStart` credential is written (`remember`) only after the bridge has started. A failed write is
logged as `log::error!`, naming `instance:target` and never the secret, and the started stream is
still returned. A failure to record a refusal is logged the same way.

**The remembered record.** It is a `screen-sharing` record in the operator's own vault, under the
account `host:<daemon_instance_id>:<target_id>`, labelled `<target> on <instance>`. The secret is
the password. The metadata holds `method` (the `DesktopAuthMethod` name), `username`, `domain`, and
`refused` once refused. `SessionVaultTargetStore::list` skips every `host:` account, so remembered
host credentials never show up as session targets. A vault that is not open has nothing usable
remembered: `usable` answers `None` and the operator is asked.

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
the host-scoped calls (host desktops are prompted for, and kept only on Remember — see above), and VNC input.
`start_stream` reads a target and its password from the store and hands the password to the bridge on
its stdin.

## Tests

| Suite | Holds still |
|---|---|
| `tests/screen_sharing_records_unit.rs` | the record mapping, malformed records, the AEAD tamper |
| `tests/screen_sharing_service_acceptance.rs` | the service, over an in-memory store that uses the real mapping |
| `tests/bridge_readiness_acceptance.rs` | a start against a `/bin/sh` stand-in bridge scripted per test (`tests/common`): refused sign-in, reported size on both scopes, early exit, silence past the timeout, and that a failed bridge is stopped |
| `tests/x11_channels_acceptance.rs` | an X11 target through the RPC against stand-in `x11vnc` and bridge scripts (`tests/common`): argv `-display :1 -localhost`, the bridge pointed at the x11vnc's port, stop on close, one x11vnc after a reopen (also when the old bridge exits with its x11vnc), never-accepts → `DeadlineExceeded` with no bridge and the x11vnc stopped, and stop when the bridge fails to start or exits after running. The stand-ins are run once with `--warm-up` before any test clock starts, because a new executable's first exec on a loaded macOS machine can outlast a short accept window |
| `tests/host_desktop_sign_in_acceptance.rs` | inspection (the methods its bridge read, only `{host, port}` on its stdin, an X11 display answered `NONE` with no bridge run, a failing bridge → `Unavailable` with its reason, `remembered`), the `DESKTOP_CREDENTIALS` question, the domain on the bridge's stdin, and Remember over the operator's real vault: no question on reopen, sealed only after the desktop accepts it, written nowhere without Remember, a refused credential never remembered, a refused remembered one never reused, a locked vault still asks, and Remember with a locked vault refused before the bridge starts |
| `tests/session_vault_target_store_acceptance.rs` | the store, over a real sealed vault: cross-session listing, locked, cleartext leakage |
| `tests/screen_sharing_record_propagation.rs` | reconciliation of a screen-sharing record through the sync engine |
| `tddy-service/tests/screen_sharing_carries_no_passphrase.rs` | the schema declares no unlock RPC and no passphrase field |
