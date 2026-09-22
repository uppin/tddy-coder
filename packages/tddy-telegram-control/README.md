# tddy-telegram-control

The Telegram control plane: everything that turns a chat into a session front-end on top of the
daemon's session lifecycle. Inbound commands and callbacks, the session watcher that turns presenter
events into chat messages, and Telegram's two sinks — the presenter-event sink and the
session-notification subscriber.

**This crate owns the `teloxide` long-polling dispatcher.** The workspace's other `teloxide`
users are the transport in [`tddy-telegram`](../tddy-telegram/) and `tddy-daemon`'s `runtime.rs`,
which builds the `Bot`. `tddy-session-lifecycle` does not declare it.

## Where it sits

```
tddy-telegram ──────────┐
                        ├──▶ tddy-telegram-control ◀── tddy-daemon (runtime.rs, server.rs)
tddy-session-lifecycle ─┘
```

It depends on both [`tddy-telegram`](../tddy-telegram/) (the transport and the chat-side state:
`TelegramSender`, the elicitation lease, the tracked-session gate) and `tddy-session-lifecycle` (the
session machinery the control plane drives). It cannot live in either. `tddy-telegram` does not
depend on `tddy-session-lifecycle`, which depends on it, so the control modules there would close a
cycle, and `tddy-session-lifecycle` holding them is the cycle this crate was cut to remove.

**Neither crate may depend back on this one.** `tddy-session-lifecycle` reaches Telegram only
through the `PresenterEventSink` port in
[`tddy-daemon-kernel`](../tddy-daemon-kernel/docs/daemon-kernel.md), which `tddy-daemon` injects.
`packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs` parses both manifests and fails on
either back-edge.

## Module layout

| Module | Owns |
|---|---|
| `telegram_bot` | the long-polling dispatcher; routes inbound messages and callbacks to the harness |
| `telegram_session_control` | `TelegramSessionControlHarness` — pickers, session start, elicitation answers, chaining, the session list — in seven files |
| `telegram_notifier` | `TelegramSessionWatcher`: the metadata tick, the presenter `ServerMessage` surface, every keyboard-bearing message |
| `telegram_multi_select_shortcuts` | the *Choose none* / *Choose recommended* callbacks |
| `telegram_session_subscriber` | `TelegramDaemonHooks` and its `PresenterEventSink` implementation |
| `telegram_notification_subscriber` | `TelegramNotificationSubscriber`, Telegram's subscriber on the session-notification bus |

Layout of `telegram_session_control/`, the sink wiring and the log targets:
[docs/architecture.md](docs/architecture.md).

## Consumers

`tddy-daemon` only: `runtime.rs` builds the hooks, the harness, the bot and the bus subscriber, and
`server.rs` sends the daemon lifecycle message. There is **no facade** at the old
`tddy_session_lifecycle::telegram_*` paths, and `tddy-daemon` never re-exported them.

## Tests

`tests/` holds the 15 suites that exercise this code: the 12 Telegram acceptance and integration
suites, and `session_notifications_acceptance`, `session_chaining_phase2_acceptance` and
`session_chaining_phase2_unit`. `./test -p tddy-telegram-control` runs them.

Open debt is in [docs/code-issues/](docs/code-issues/): the two untested bot handlers, five
over-budget `telegram_session_control` modules, and `telegram_notifier.rs`.

Features: [telegram-session-control.md](../../docs/ft/daemon/telegram-session-control.md),
[telegram-notifications.md](../../docs/ft/daemon/telegram-notifications.md).
