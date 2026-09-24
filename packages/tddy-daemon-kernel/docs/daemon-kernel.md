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
- **A module, as a rule.** `config.rs` is the single exception, and a deliberate one: four moving
  modules and every handler in both new services take `&DaemonConfig` and read disjoint parts of it,
  so there is no smaller cut — the symbol *is* the file. The alternative, a narrow value struct per
  consuming crate, is authoring rather than moving, and every later node would repeat it.
- **Anything the moving families do not reach.** `pty_registry.rs` was moved here and then retracted
  untouched on exactly that test.
- **The daemon's signing key and key directory.** They are auth's (`tddy-daemon-auth`), and they
  must not land here even if several crates reach them: `peer_forwarding.rs` makes this crate carry
  the LiveKit SDK, which cannot enter `tddy-tools`' `--no-default-features` in-jail build, so
  anything placed here inherits that unreachability.
- **The participant-identity rule.** `SPLIT_AGENT_IDENTITY_PREFIX` and the other non-daemon
  prefixes are defined in `tddy_service::participant_identity`, beside
  `may_be_daemon_discovery_identity`, because the kernel depends on `tddy-service` and the rule must
  be one both the mint and discovery read. `daemon_identity` re-exports the split-agent prefix for
  the crates that mint agents' identities.

## Ports

### `presenter_observer::PresenterEventSink`

```rust
#[async_trait]
pub trait PresenterEventSink: Send + Sync {
    async fn on_presenter_event(&self, session_id: &str, event: &ServerMessage)
        -> anyhow::Result<()>;
}
pub type SharedPresenterEventSink = Arc<dyn PresenterEventSink>;
```

Where a workflow session's presenter events go besides the session-notification bus.
`tddy-session-lifecycle`'s `DaemonSessionHost` holds `Option<SharedPresenterEventSink>`, and its
`presenter_observer_task` calls the sink for each event of the child's `PresenterObserver` stream, in
stream order. An error from the sink ends that session's observer loop, as a failed stream read
does. The one implementation is `TelegramDaemonHooks` in
[`tddy-telegram-control`](../../tddy-telegram-control/docs/architecture.md), which `tddy-daemon`'s
`runtime.rs` injects when a bot is configured.

**The port is the sink, not the observer.** The observer feeds two independent consumers: this
sink and the notification bus that lights a session's drawer indicator in `tddy-web`. It runs when
either exists. A port for the whole observer, owned by Telegram, would take the bus publish with
it and leave the indicator dark on every daemon without a `telegram:` block. So the loop — connect
with retry, stream, bus publish, the "neither sink, do not spawn" rule — stays in
`tddy-session-lifecycle`, and only the Telegram half is behind the trait.

**Absence is `None`, and there is no no-op implementation.** The spawn rule has to know that
there is no sink. A no-op sink would start an observer on a daemon with nothing to deliver to.

`tests/telegram_extraction_shape.rs` proves delivery through the shared port with a recording sink
of its own, and pins the shape of the cut around it: `connection_service` names no Telegram symbol,
its one consumer goes through the port, `tddy-session-lifecycle` declares no `teloxide` and holds no
`telegram_*` module, `tddy-telegram-control` depends on `tddy-telegram` and
`tddy-session-lifecycle` with no edge back from either, and no `telegram_session_control` module
exceeds 800 production lines. The suite parses manifests with `toml` (a dev-dependency) and checks
every dependency table, so a comment naming a crate is not mistaken for a dependency on it.

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
The ceiling is `tddy_github::REFRESH_TOKEN_TTL`, and this crate, with seventeen dependents, does
**not** depend on `tddy-github`; the meaning lives in a crate that already does.

`pending_login_ttl.rs` and `open_vault_idle_ttl.rs` now hold only their module doc and the parsing
tests (absent, `0`, a value past the ceiling read as given, and the two refusals naming the
setting), so `config.rs` — far over its size budget — carries each field and a one-line pointer.

## See also

- [`packages/tddy-daemon/docs/connection-service.md`](../../tddy-daemon/docs/connection-service.md)
- [`docs/ft/daemon/host-worktree-services.md`](../../../docs/ft/daemon/host-worktree-services.md)
- [changesets/](./changesets/)
