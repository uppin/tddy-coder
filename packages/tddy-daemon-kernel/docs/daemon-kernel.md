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

## See also

- [`packages/tddy-daemon/docs/connection-service.md`](../../tddy-daemon/docs/connection-service.md)
- [`docs/ft/daemon/host-worktree-services.md`](../../../docs/ft/daemon/host-worktree-services.md)
- [changesets/](./changesets/)
