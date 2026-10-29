# Daemon kernel (tddy-daemon-kernel)

The symbols every `tddy-daemon` subsystem reaches into `connection_service` for, in a crate a
subsystem can depend on without depending on the daemon.

The crate's own rustdoc (`src/lib.rs`) is the reference: what is here, who reached for it, and the
de-duplication each symbol went through. This page is the **admission rule** — what belongs in the
kernel and what does not — because that is the question every later `#unbundle` node asks.

## Why a crate and not a wider visibility

**`pub(crate)` does not cross a crate boundary.** A module split is served by widening a field; a
crate split is not served by it at all. Once the consumer is a different crate the shared surface has
to be a crate of its own, which is what makes a subsystem move mechanical rather than impossible.

## What belongs here

| Admitted | Rule |
|---|---|
| A **symbol** several subsystems reach for | the default. The unit is the transitive call closure, measured before the move — `spawn_as_user` is 179 of `spawner.rs`'s 2,539 lines; `privilege_drop` 63 of 400; `user_paths` 34 of 210 |
| A symbol that exists **more than once** | consolidating it is a behaviour decision made once, here, and stated. `now_unix_ms` saturates because the alternatives were an `i64` refusal and a bare `as u64` cast that truncates |
| A **port** a subsystem is injected through | when a service would otherwise name a concrete type from a crate that depends on it. The trait goes here, the implementation stays with the subsystem, and the composition root in `tddy-daemon` injects it — see [Ports](#ports) |

Every origin module re-exports every lifted name, so no caller in `tddy-daemon` changes and there
stays exactly one definition of each.

## What does not

- **A subsystem.** The model registry, telegram, screen sharing, sandbox, spawn, auth and LiveKit are
  services in their own right and belong to their own crates.
- **A module, as a rule.** `config` is the first exception, and a deliberate one: four moving
  modules and every handler in both new services take `&DaemonConfig` and read disjoint parts of it,
  so there is no smaller cut at the crate level — the symbol *is* the module. Inside the crate it is a
  module directory (see [`config` layout](#config-layout)). The alternative, a narrow value struct per
  consuming crate, is authoring rather than moving, and every later node would repeat it. The other
  two are whole leaf modules with no session-host state, which the host and the RPC families above
  it both reach, and which no subsystem crate owns: `relay_idle` (`RpcActivity`, the handle every RPC
  handler bumps so a relay daemon does not shut down mid-session, over `tddy_task::IdleTimeoutTracker`)
  and `local_token_tonic_adapter` (`LocalTokenUdsTonicAdapter`, which serves `MintLocalToken` from the
  Unix socket's peer credential). They are why this crate depends on `tddy-task`, `tddy-github` and
  `tonic`, and `tddy-session-lifecycle` re-exports both under their old paths.
- **Anything the moving families do not reach.** `pty_registry.rs` was moved here and then retracted
  untouched on exactly that test.
- **The daemon's signing key and key directory.** They are auth's (`tddy-daemon-auth`), and they
  must not land here even if several crates reach them.
- **The participant-identity rule.** `SPLIT_AGENT_IDENTITY_PREFIX` and the other non-daemon
  prefixes are defined in `tddy_service::participant_identity`, beside
  `may_be_daemon_discovery_identity`, because the kernel depends on `tddy-service` and the rule must
  be one both the mint and discovery read. `daemon_identity` re-exports the split-agent prefix for
  the crates that mint agents' identities.

## Ports

### `operator_channel::OperatorChannel`

```rust
#[async_trait]
pub trait OperatorChannel: Send + Sync {
    fn id(&self) -> &'static str;
    fn follows_presenter_events(&self) -> bool { true }
    async fn on_presenter_event(&self, session_id: &str, event: &ServerMessage)
        -> anyhow::Result<()>;
    fn wants_notification(&self, notification: &SessionNotification) -> bool;
    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()>;
}
pub type SharedOperatorChannel = Arc<dyn OperatorChannel>;
pub struct NoOperatorChannel;
pub fn awaits_operator(mode_changed: &ModeChanged) -> bool;
```

**A chat an operator is reached on.** A session reaches its operator two ways, and both end at this
one port:

| Path | Methods | Fed by |
|---|---|---|
| **Presenter events** — a workflow session's presenter stream, in stream order (elicitations, state lines) | `on_presenter_event` | `tddy-session-activity`'s `presenter_observer_task`, for each event of the child's `PresenterObserver` stream |
| **Notifications** — the attention-worthy events off the session-notification bus | `wants_notification`, then `on_notification` | `tddy_session_activity::session_notifications::OperatorChannelSubscriber`, the bus adapter that registers a channel as a subscriber named after `id()` |

`id()` is what the channel logs under and reports on the integration status board. An error from
`on_presenter_event` ends that session's presenter observer, as a failed stream read does; an error
from `on_notification` is logged by the bus and never reaches the RPC that published it.
`wants_notification` is asked first, so a channel that sends over a network is never handed what it
would only discard.

`tddy-session-lifecycle`'s `DaemonSessionHost` holds a `SharedOperatorChannel` (the
`operator_channel` field), and `tddy-daemon`'s `runtime/mount_host.rs` installs it twice from the
same value — on the host, and on the daemon's bus through `OperatorChannelSubscriber`. The one
implementation is `TelegramDaemonHooks` in
[`tddy-telegram-control`](../../tddy-telegram-control/docs/architecture.md), installed when a bot is
configured.

**Absence is the named null object `NoOperatorChannel`**, the extensions convention
([extension-ports.md](../../tddy-daemon-extensions/docs/extension-ports.md)). It wants no notification
and accepts every presenter event, and its `follows_presenter_events()` answers `false`. That answer
is what the observer's spawn rule reads: the observer runs when a channel follows presenter events
**or** a notification bus wants them, and is not started when neither does. The bus is what lights a
workflow session's drawer indicator in `tddy-web`, so the observer — connect with retry, stream, bus
publish — runs on every daemon, Telegram or not, and only the chat half is behind the trait.

**The core owns the presenter-gate rule.** `awaits_operator(&ModeChanged)` is the one answer to
"this mode waits on a human": document review, markdown viewer, feature input, select, multi-select
and text input do; `Running`, `Done` and a mode change naming no mode do not. The drawer indicator
turns a session's dot yellow on it, and Telegram opens and closes its per-chat elicitation queue on
it, so the two cannot disagree about a gate. It is here rather than in a channel because it must hold
on a daemon with no channel at all.

**Why the kernel and not `tddy-daemon-extensions`.** The port carries `tddy-service`'s
`ServerMessage`, and the extensions crate stays free of `tddy-service`. The integration status board
the Telegram channel reports on stays in extensions.

### `session_notification` — the types the port names

`SessionNotification`, `SessionNotificationKind` (`Activity`, `AttentionRequired`),
`SessionNotificationSource` (`ActivityStatus`, `AgentToolCall`, `Presenter`, `Agent`) and an agent
notification's content (`AgentNotification`, `NotificationLevel`) are defined here so the port can
name them. The bus, its subscriber trait and its subscribers live in `tddy-session-activity`,
whose `session_notifications` module re-exports these three under its own path —
[session-notifications.md](../../tddy-session-activity/docs/session-notifications.md).

### Tests

`tests/operator_channel_acceptance.rs` pins the gate rule over every mode that asks a question and
every autonomous one, the null object's answers (wants nothing, follows no presenter stream, accepts
every event), the trait's default for `follows_presenter_events`, and delivery through the shared
port.

`tests/telegram_extraction_shape.rs` pins the shape of the cut around the port:
`connection_service` names no Telegram symbol and holds the operator-channel port, its one consumer
goes through the port, `tddy-session-lifecycle` declares no `teloxide` and holds no `telegram_*`
module, `tddy-telegram-control` depends on `tddy-telegram` and `tddy-session-lifecycle` with no edge
back from either, and no `telegram_session_control` module exceeds 800 production lines. The suite
parses manifests with `toml` (a dev-dependency) and checks every dependency table, so a comment
naming a crate is not mistaken for a dependency on it. The workspace-wide rule — no crate but
Telegram's own and the daemon's wiring names a Telegram crate — is the ratchet in
`tddy-daemon-extensions` ([extension-ports.md](../../tddy-daemon-extensions/docs/extension-ports.md#telegram-the-operator-channel-and-its-ratchet)).

## Users, the live holder and first-login enrolment

### `LiveUsers` — `users:` held once per running daemon

`DaemonConfig.users` is a `LiveUsers` (`src/live_users.rs`), not a `Vec<UserMapping>`. It is a
cheap-to-clone handle on one `Arc`'d set of rows, so **cloning a `DaemonConfig` shares `users:`
rather than copying it**. The daemon hands each of its services a clone of one loaded config, and
every one of them therefore reads the same rows: a row enrolled through any of them is seen by all of
them at once, with no restart.

It serialises and deserialises as the plain list it always was (`users:` in YAML, omitted when
empty), so no config file changes shape.

| Method | What it does |
|---|---|
| `os_user_for_github(login) -> Option<String>` | the mapped OS user, or `None`. **No default arm** |
| `mapping_for_os_user(os_user)` | the row for the local peer-trust path that starts from a uid |
| `first_github_user()`, `is_empty()` | whether, and to whom, the deployment is enrolled |
| `enrol_first_login(config_path, github_user, os_user)` | the one mutation — below |
| `while_rewriting_config_file(rewrite)` | runs another rewrite of the config file under the same lock as enrolment |

`DaemonConfig::os_user_for_github` delegates to it. Its **behaviour is unchanged** — a linear
search, `None` for an unmapped login, no default arm — and it returns `Option<String>` because a
borrow cannot escape the rows' lock.

⚠ **A sharp edge, documented rather than removed.** Because clones share the rows, code that clones
a `DaemonConfig` to build a *different* configuration — a test fixture, a variant for a second
daemon — shares `users:` with the original, and an enrolment through either is seen by both. Build
an independent holder (`LiveUsers::new(rows)`) where independence is meant.

Two locks, deliberately separate: `rows` (an `RwLock`) for lookups, and `file_writes` (a `Mutex`)
held across every rewrite of the config file. A lookup never waits on file I/O. The two writers of
that file — enrolment and `DaemonConfigService`'s `UpdateConfig`, which re-serialises the whole
config, `users:` included — both take `file_writes`, so an update that read the rows before an
enrolment and wrote after it cannot persist the file without the enrolled row.

### `enrol_first_login` — the one-time write

A desktop install has nobody to write `users:`. Its first login, from its own window, is written
down instead (`tddy-daemon-auth`'s `FirstLoginEnrolment` decides *when* —
[auth-service.md](../../tddy-daemon-auth/docs/auth-service.md#first-login-enrolment)). The kernel
owns *how*:

- **`LiveUsers::enrol_first_login`** takes `file_writes`, re-checks that no row exists inside it,
  writes the file, and only then pushes the row into memory. Of two first logins racing, exactly one
  is enrolled and the other is refused `AlreadyEnrolled`. When the write fails nothing is applied,
  so a daemon never admits a login it could not record.
- **`first_login_enrolment::enrol_first_login`** is the file half. It re-reads the config from disk
  (the file, not the caller's snapshot, decides whether somebody is already enrolled), then edits the
  document as a `serde_yaml::Value`, inserting only `users:` so every other key stays as the operator
  wrote it, and writes it back atomically (`write_atomic_labelled`) — a half-written config is a
  daemon that will not start.

`EnrolmentRefusal`:

| Variant | When |
|---|---|
| `AlreadyEnrolled { github_user }` | `users:` already names somebody. Enrolment is once per deployment; a second account is added deliberately, never by signing in |
| `ConfigNotWritable { reason }` | the file cannot be read, parsed as a mapping, or rewritten — read-only, owned by another account, or gone |

**Enrolment is not a fallback for the lookup, and must not become one.** A default arm in
`os_user_for_github` would answer "no mapping" every time, for every GitHub account on earth.
Enrolment writes the row *once*, on a deployment that has none, and the unchanged lookup then finds
it; a second, different login on an enrolled deployment is refused exactly as on a server. Adding a
second account deliberately is `#keyring` 8/9 ([#515](https://github.com/uppin/tddy-coder/pull/515)).

**Known limits.**

- **Comments are lost.** Serialising the `Value` back strips every YAML comment, so on a desktop's
  first sign-in the explanatory header `./install --desktop` rendered is gone from
  `~/.tddy/desktop.yaml`. `UpdateConfig` has the same loss. Both need one comment-aware YAML editor,
  tracked in the backlog (`docs/dev/todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`),
  with a `TODO` at the write.
- The free function `first_login_enrolment::enrol_first_login` is public and does **not** take
  `LiveUsers`' file lock. Its only production caller is `LiveUsers::enrol_first_login`; call that.
- The file write is synchronous, on whatever task calls it. It happens once per deployment.

`tests/first_login_enrolment_acceptance.rs` pins the first login written down, a second account
refused, the rest of the config kept, and an unmapped login still resolving to nobody.

## `pending_login_ttl`, `open_vault_idle_ttl` — two settings read here and meant elsewhere

`GitHubConfig.pending_login_ttl_seconds` (how long a sign-in's GitHub token may wait in memory for
its credential vault, and so how long that sign-in may choose the vault's passphrase) and
`GitHubConfig.open_vault_idle_ttl_seconds` (how long an open vault may go unused before the daemon
closes it) are both a plain **`Option<u64>`** of seconds, `#[serde(default)]`. This crate reads
them and nothing more:

- absent → `None`; a whole number → `Some(n)`, **including `0` and values past any limit**;
- a negative or non-numeric value fails the config load, and serde_yaml names the field path
  (`github.open_vault_idle_ttl_seconds: invalid type: …`).

**What the numbers mean is `tddy-daemon-auth`'s** (`vault_lifetimes::VaultLifetimes::of`, where
the credential vaults are built): the defaults (600 s; the refresh-token lifetime), the ceiling (the
refresh-token lifetime, past which the **daemon does not start**) and `0` = never — see
[auth-service.md § Credential vaults](../../tddy-daemon-auth/docs/auth-service.md#credential-vaults).
The ceiling is `tddy_github::REFRESH_TOKEN_TTL`, and the meaning lives where the vaults are built,
not in a crate with seventeen dependents that only parses the setting.

`GitHubConfig.dev_vault_without_passphrase` (`Option<bool>`, `#[serde(default)]`) is read here the same
way and meant in `tddy-daemon-auth` too: it opens every vault under the empty passphrase on a stub
daemon, and stops any other daemon at startup — see
[auth-service.md § Development vaults without a passphrase](../../tddy-daemon-auth/docs/auth-service.md#development-vaults-without-a-passphrase).

`pending_login_ttl.rs` and `open_vault_idle_ttl.rs` now hold only their module doc and the parsing
tests (absent, `0`, a value past the ceiling read as given, and the two refusals naming the
setting), so `config.rs` carries each field and a one-line pointer.

## `peer_forward_timeout_secs` and `CommonRoom` — how long a forward to a peer waits

`DaemonConfig.peer_forward_timeout_secs` (`u64`, default **30**, taken from
`peer_forwarding::PEER_FORWARD_TIMEOUT`) bounds how long a daemon waits for a peer in the common room
to answer a forwarded unary RPC, or to open a forwarded server stream. `peer_forward_timeout()` returns
it as a `Duration`, clamped to at least one second, so `0` means one second (as for
`spawn_worker_request_timeout_secs`). An absent key means 30 s.

`peer_forwarding::CommonRoom` is the handle every forwarding call goes through. It holds an
`Arc<dyn PeerForwarder>` — the transport, a port defined in `tddy-daemon-extensions` — and that
deadline, built once with `CommonRoom::new(forwarder, &config)`; `is_connected()` says whether a peer
can be reached at all, and `forward_timeout()` gives the deadline. `forward_to_peer`,
`forward_to_peer_within` (an explicit deadline, for a call the peer spends minutes serving) and
`forward_server_stream_to_peer` are its methods. The deadline rides with the handle because it is the
one thing every call site already holds, and because a process-wide value would be shared by the
several daemons the e2e tests run in one process.

The kernel owns the forwarding **policy** and the transport owns the **carrying**: every transport
gets the same deadline, the same stream idle rule and the same errors, because they are applied here
rather than inside each implementation. The kernel names no LiveKit crate; the LiveKit integration
implements the port (see [`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/livekit-service.md)),
and a daemon with no transport wired holds `NoPeerForwarding`, for which `is_connected()` is false and
every forward is a `FailedPrecondition` naming the call and the peer.

- The **open** of a forwarded stream waits `forward_timeout()`. The idle wait **between frames** of a
  forwarded stream is the fixed `PEER_FORWARD_STREAM_IDLE_TIMEOUT` (30 s), not a setting: it is held
  above the roster's service threshold by a test and below `ROSTER_KEEPALIVE_INTERVAL * 2` by a
  compile-time assertion.
- The loop that carries a forwarded stream's frames is `stream_relay::Relay { idle_timeout, decode_frame,
  idle_status }` with `run(frames, tx)`, separate from `CommonRoom` so the idle policy runs without a LiveKit
  room. The timeout is **per frame**: every frame restarts it, so a peer emitting keep-alives while it waits
  on a clone is never cut off, and one that goes silent ends the relayed stream with an error, never a short
  success.
- A timeout is `DeadlineExceeded` naming the seconds waited. A peer that has left the common room is
  still refused up front (`refuse_departed_daemon`), never waited out.
- A relayed stream ends at the first terminal frame: a peer's error frame is relayed once, and a frame
  that cannot be decoded ends the relay with that error. A consumer that drops its receiver releases the
  transport's stream no later than the idle timeout.
- Every start forwarded to a peer — the remote managed worktree's split start, the agent-clone provision
  and the plain `StartSession` forward — is a **relayed stream with no overall deadline**:
  `peer_calls::forward_start_session_relaying(room, peer, req, relay)` opens `StreamStartSession`, hands
  every non-terminal `StartSessionEvent` to `relay` as it arrives, and returns the peer's result. The open
  waits `PEER_FORWARD_STREAM_IDLE_TIMEOUT`, not `peer_forward_timeout_secs`, and each frame restarts the
  idle window; the peer's keep-alives (every 5 s for a start's whole life) keep a live start open.
- A start forward's failure is `ForwardedStartFailure::{PeerAnswered(Status), PeerSilent(Status)}`,
  decided by the relay rather than by a status code: `stream_relay::RelayFailure::Peer` for any status the
  peer sent (its own `DEADLINE_EXCEEDED` included) and for an undecodable frame, `Silent` only for the idle
  expiry (`CommonRoom::forward_server_stream_classified`). A refusal the transport returns before the
  stream opens is `PeerAnswered`; only silence is `PeerSilent`. The other stream callers keep a plain
  `Status` through the same loop.

## `peer_calls` — typed RPCs forwarded to a peer

`peer_calls` holds the fourteen typed forwards over `CommonRoom`: each encodes its request, forwards it
to the peer's service and method, and decodes the answer. The unary ones go through one private
`forward_unary`; a body that does not decode is `Internal`, `"decode <ResponseType>: …"`.

| Family | Functions | Service |
|---|---|---|
| session | `forward_start_session_relaying` (relayed start, `ForwardedStartFailure`), `forward_stream_start_session`, `forward_delete_session` | `session.SessionService` |
| exec tool | `forward_stream_execute_tool` | `exec_tools.ExecToolService` |
| tasks | `forward_cancel_task` (a split start's codebase half; a `CancelTask` addressed to a peer) | `tasks.TaskService` |
| project | `forward_add_project_to_host`, `forward_set_project_default_branch`, `forward_set_project_accounts` | `project.ProjectService` |
| session files | `forward_upload_staged_attachment_chunk`, `forward_list_staged_attachments`, `forward_delete_staged_attachment`, `forward_read_host_document`, `forward_stream_read_host_document` | `tddy_service::SESSION_FILES_SERVICE` |

They are tested over a scripted `PeerForwarder` (`tests/peer_calls.rs`): the service, method and
encoded body carried, a garbage answer as `Internal`, and a stream relayed frame by frame. The LiveKit
plugin re-exports each under its old `*_via_livekit` name for the callers that have not moved.

## `keyring.group_secret` — which peers may hold this daemon's credentials

`DaemonConfig.keyring: Option<KeyringConfig>` is `#keyring` 6/9's one setting:
`KeyringConfig { group_secret: Option<String> }`. Absent means this daemon propagates credentials to
**nobody** — not "to everyone in the common room". Room membership carries no attestation that a
participant runs `tddy-daemon` (`docs/ft/daemon/livekit-peer-discovery.md` § *Trust model*), so
handing out a person's GitHub credential to anything holding the room's LiveKit credentials is the
property `tddy-credential-sync` exists to remove — and a daemon with no fleet (a desktop install,
most of the time) is the common case for leaving this unset.

The secret itself never travels: a peer proves it holds the same one by an HMAC over a fresh
challenge (`GroupSecret::proof_for`/`verifies` in `tddy-credential-sync`), never the secret in the
clear. It is one of two independent checks a peer must pass before it receives anything — the other
is `#keyring` 1/9's Ed25519 signature — and this crate carries only the configuration value; the
checks themselves live in `tddy-credential-sync`, which this crate does not depend on.

## `screen_sharing` — where the bridges and x11vnc are found

`ScreenSharingConfig` holds `vnc_binary_path`, `rdp_binary_path`, `x11vnc_binary_path` and
`x11_socket_dir`, every one an empty string by default. `resolve_vnc_binary_path` and
`resolve_rdp_binary_path` try the explicit path, then a sibling of `current_exe()`, then the bare
name on `PATH`. `resolve_x11vnc_binary_path(&ScreenSharingConfig)` has no sibling step: x11vnc is a
system package and never shipped beside tddy, so an unset path is `x11vnc` on `PATH`. An empty
`x11_socket_dir` means `/tmp/.X11-unix`, which `tddy-host-service`'s `x11_displays` applies. None of
these resolvers checks that the file exists; the tooling probe does that
(`binary_is_present`). `tests/x11vnc_binary_resolution.rs` holds the x11vnc resolver.

## `notification_markdown` and `agent_notification_limits` — an agent notification's body

Kernel-owned because more than one surface renders the same agent-written body:

- `notification_markdown::to_telegram_html(md)` — `pulldown-cmark` events to Telegram's HTML subset:
  `b`, `i`, `code`, `pre`, `a` (headings bold, list items as `•` lines); every other construct,
  raw HTML included, is escaped text. `to_plain_text(md, max_chars)` flattens the markup and cuts at
  a character boundary with `…`; `escape_html(text)` escapes `& < > "` for the words that are not
  markdown (a title, a label).
- `agent_notification_limits::validate_agent_notification(title, body)` — title 1–`MAX_TITLE_CHARS`
  (200) characters after trimming, body at most `MAX_BODY_CHARS` (2,000); the `Err` names the part
  that is too long, worded for the agent that sent it.

Tests: `tests/notification_markdown_unit.rs`, `tests/agent_notification_limits_unit.rs`.

## `agent_list_mapping` and `daemon_hook_urls` — two daemon-wide helpers with no session state

`agent_list_mapping` is the daemon-facing adapter over `tddy_discovery::agent_list_mapping`: it maps the
daemon's `DaemonConfig` agent definitions to the `AssistantEntry` list a client reads.

`daemon_hook_urls` holds `advertise_daemon_url`, `local_daemon_hook_url`, `claude_hook_daemon_url` and
`DEFAULT_WEB_PORT`: where a hook command reaches this daemon (its own web listener on loopback when nothing
is configured) and the URL it advertises to peers. The port default is defined here and nowhere else, since
a hook posting to the wrong port fails silently. Each is a function of the config alone.

Both are re-exported at their old `tddy_session_lifecycle::…` paths.

## `os_user_resolution` — a session token to its OS user

`resolve_os_user(config, user_resolver, session_token)` maps a caller's session token to the OS user the
daemon runs its work as: `Unauthenticated` when the resolver knows no such session, `PermissionDenied`
when the GitHub user has no OS user in `config`. It is a free function so a family handler above this
crate authenticates a caller exactly as the session host does, without holding the host.
`tddy-session-lifecycle` re-exports the module at its old path.

## `privilege_drop` and `pre_exec_stages` — starting a child as another OS user

Every daemon spawn as another OS user — `tddy-spawn`'s `clone_as_user_with_env` and `spawn_as_user`,
and this crate's `spawn_as_user::as_user_command` (behind `run_capture_as_user`, `run_output_as_user`,
`start_output_as_user`) — goes through one path:

1. **`resolve_target_account(os_user) -> TargetAccount`**, in the parent: the passwd entry (`name`, `uid`,
   `gid`, `home`) and the supplementary group list from `getgrouplist(3)` — the NSS walk `initgroups(3)`
   would otherwise do after the fork. A failure to enumerate the groups is an error, never a short list.
2. **`plan_privilege_change(target) -> Result<PrivilegeChange, CannotBecomeUser>`**, still before any
   fork, comparing **effective** ids (`EffectiveIdentity::current()`; `plan_privilege_change_from` takes
   the identity explicitly for tests). Same uid **and** gid → `PrivilegeChange::None`. Root →
   `Drop(target)`. Anything else is refused, including a matching uid with another gid, which could not
   `setgid` either. `CannotBecomeUser`'s message names both users and says a supervised host brokers
   such spawns through `tddy-supervisor`.
3. **`StagedCommand::new(command, label, change).spawn() -> StagedChild`** installs the `pre_exec` and
   spawns. The child makes only async-signal-safe calls: it writes a 5-byte record (stage byte + its
   pid, little-endian) to a close-on-exec pipe **before** each step — `Forked`, then for a `Drop`
   `Groups` / `Gid` / `Uid` around bare `setgroups` / `setgid` / `setuid` — and `Exec` last, immediately
   before `execve`. No environment writes (`HOME` is set on the `Command`, before the fork), no NSS, no
   allocation.
4. A watcher thread runs **`watch_stages(reader, label, heartbeat) -> StageOutcome`**: it logs
   `spawn <label> pid=<n> reached <stage>` per record and, while a step is outstanding,
   `spawn <label> pid=<n> still at <stage> after <n>s` every `STAGE_HEARTBEAT` (5 s). A successful exec
   closes the pipe; EOF after `Exec` is `Execed`, earlier is `StoppedAt(stage)`, none is
   `NothingReported`. The thread ends by itself at EOF; callers that do not need the outcome drop the
   handle.

Things a reader of the code would otherwise rediscover:

- **Every spawn forks, same-user children included.** `pre_exec` forces std's fork path (no
  `posix_spawn`); the closure is async-signal-safe, so that is safe in the multi-threaded daemon, and it
  is what makes a same-user child report `forked` → `exec` too.
- **A record names the step being entered, not the one finished**, so a child stuck inside `setgroups`
  shows `still at groups`.
- **An `execve` that fails after the `Exec` record** is reported by the watcher as `Execed` while
  `spawn()` returns the exec error — the `Command`'s own error is authoritative; the stage log says only
  that the child got as far as calling exec.
- **A non-root daemon now gets `CannotBecomeUser` before forking** instead of an `EPERM` from the child.
  `tddy-host-service`'s key and tooling probes surface it as their error string; `tddy-daemon-rpc`'s model
  probe as `FailedPrecondition("model probe failed: …")`.
- On macOS the pipe is made close-on-exec with `fcntl` after `pipe(2)` (there is no `pipe2`), as std's
  own pipes are there.

Tests: `tests/privilege_change_acceptance.rs` (plan and refusal, account resolution) and
`tests/pre_exec_stages_acceptance.rs` (stage order of a real same-user child, the heartbeat on a
test-owned pipe, record encoding). `tddy-spawn/tests/pre_exec_safety.rs` scans the spawn paths for
`set_var` / `initgroups`.

## See also

- [`packages/tddy-daemon/docs/connection-service.md`](../../tddy-daemon/docs/connection-service.md)
- [`docs/ft/daemon/host-worktree-services.md`](../../../docs/ft/daemon/host-worktree-services.md)
- [changesets/](./changesets/)

## `config` layout

`config.rs` holds `DaemonConfig` with its `Default` and its field-level `impl`,
`TelegramConfig`, `ListenConfig`, `LiveKitConfig`, the serde defaults those name, and the facades below.
About 440 production lines (production code excluding `#[cfg(test)]` items). Eight modules under
`src/config/` hold the rest:

| Module | Holds |
|---|---|
| `agent_cli` | `ClaudeCliConfig`, `CursorCliConfig`, their defaults, every `resolve_claude_*` / `resolve_cursor_*`, the `CLAUDE_*` / `CURSOR_*` env consts, the shim and `PATH` search; `CliHarnessConfig { binary_path }`, the value type of `DaemonConfig.cli_harnesses` (`cli_harnesses: { opencode-cli: { binary_path } }`, keyed by session type), and `DaemonConfig::cli_harness_binary(session_type, default_binary, binary_env)` — the environment variable when non-blank, else the config entry when non-blank, else the default |
| `daemon_methods` | the second `impl DaemonConfig`: `load`, the accessors, `apply_*_env_override(s)`, `validate_for_relay` |
| `sections` | `RelayConfig`, `LocalConfig`, `KeyringConfig`, `GitConfig`, `SandboxCgroupConfig`, `IndexDaemonConfig` with their defaults |
| `env_overrides` | `merge_timing_env`, `merge_telegram_env` and their parsers |
| `timings` | `AgentWarmupConfig`, `SessionRoomConfig`, their defaults and consts |
| `screen_sharing` | `ScreenSharingConfig`, `resolve_{vnc,rdp,x11vnc}_binary_path` |
| `sandbox_paths` | `SANDBOX_CONFIG_*`, `sandbox_config_os_token`, `resolve_sandbox_config_path{,_with}` |
| `access` | `GitHubConfig`, `UserMapping`, `AllowedTool`, `AllowedAgent` (`pub(crate)`: in-crate code names the module) |

`config.rs` re-exports the items a caller outside the crate uses, so `tddy_daemon_kernel::config::…`
paths are unchanged for them. **Seven items have no path outside the crate:** `LocalConfig`,
`KeyringConfig`, `SandboxCgroupConfig`, `GitConfig`, `AgentWarmupConfig`, `SessionRoomConfig` and
`AllowedTool`. A caller reads them through the `DaemonConfig` fields but cannot name the types. To give one
a public path, add it to a `pub use` in `config.rs` (for example `pub use sections::{LocalConfig, …};`).
`ListenConfig` and `LiveKitConfig` stay in `config.rs` because the serde defaults they name are not contiguous
with them.

**There is no `supervisor:` section.** Whether a daemon spawns through `tddy-supervisor` follows from how it
was started (`TDDY_SUPERVISOR_SOCKET`, read by `tddy_spawn::supervisor_client::SpawnBackendChoice`), not from
config. `DaemonConfig::load` checks the untyped YAML before typed deserialization and refuses a top-level
`supervisor:` key with a message naming the block as obsolete and saying to delete it, rather than serde's bare
`unknown field` (`daemon_methods.rs`; pinned by `a_daemon_yaml_with_a_supervisor_block_is_refused_as_obsolete`).
`./install` removes the block from an existing `daemon.yaml` on upgrade.
