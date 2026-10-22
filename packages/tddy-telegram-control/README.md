# tddy-telegram-control

The Telegram control plane: everything that turns a chat into a session front-end on top of the
daemon's session lifecycle. Inbound commands and callbacks, the session watcher that turns presenter
events into chat messages, and Telegram's **operator channel** — the one port through which the
daemon's core hands a session's elicitations and attention notifications to a chat.

**This crate owns the `teloxide` long-polling dispatcher.** The workspace's other `teloxide`
users are the transport in [`tddy-telegram`](../tddy-telegram/) and `tddy-daemon`'s `runtime.rs`,
which builds the `Bot`. `tddy-session-lifecycle` does not declare it.

## Where it sits

```
tddy-telegram ──────────┐
                        ├──▶ tddy-telegram-control ◀── tddy-daemon (runtime.rs, server.rs)
tddy-session-lifecycle ─┘
```

It depends on both [`tddy-telegram`](../tddy-telegram/docs/telegram-notifier.md) (the transport and
the chat-side state: `TelegramSender`, the elicitation lease, the tracked-session gate, the status
reporting) and `tddy-session-lifecycle` (the session machinery the control plane drives). It cannot
live in either: `tddy-telegram` is the transport and drives no session, and no session crate may name
a Telegram crate.

**Telegram is a satellite.** No session crate and not the kernel depends on this crate or on
`tddy-telegram`; they reach Telegram only through the `OperatorChannel` port in
[`tddy-daemon-kernel`](../tddy-daemon-kernel/docs/daemon-kernel.md#ports), which `tddy-daemon`
installs. `packages/tddy-daemon-extensions/tests/telegram_is_a_satellite.rs` holds that over the
workspace graph, and `packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs` fails on a
back-edge from either crate below this one.

## Module layout

| Module | Owns |
|---|---|
| `telegram_bot` | the long-polling dispatcher; routes inbound messages and callbacks to the harness |
| `telegram_session_control` | `TelegramSessionControlHarness` — pickers, session start, workflow launch, elicitation answers, chaining, the session list — in ten files |
| `telegram_notifier` | `TelegramSessionWatcher`: the metadata tick, the presenter `ServerMessage` surface, every keyboard-bearing message |
| `telegram_multi_select_shortcuts` | the *Choose none* / *Choose recommended* callbacks |
| `telegram_session_subscriber` | `TelegramDaemonHooks`, the daemon's shared Telegram handles, and their `OperatorChannel` implementation |

Layout of `telegram_session_control/`, the operator channel and the log targets:
[docs/architecture.md](docs/architecture.md).

## Consumers

`tddy-daemon` only: its `runtime` builds the hooks, the harness and the bot, installs the hooks as
the operator channel, and `server.rs` sends the daemon lifecycle message. There is **no facade** at the old
`tddy_session_lifecycle::telegram_*` paths, and `tddy-daemon` never re-exported them.

## Tests

`tests/` holds the 17 suites that exercise this code: the 14 Telegram acceptance and integration
suites, and `session_notifications_acceptance`, `session_chaining_phase2_acceptance` and
`session_chaining_phase2_unit`. `./test -p tddy-telegram-control` runs them.

Open debt is in [docs/code-issues/](docs/code-issues/): the two untested bot handlers, three
over-budget `telegram_session_control` modules, and `telegram_notifier.rs`.

Features: [telegram-session-control.md](../../docs/ft/daemon/telegram-session-control.md),
[telegram-notifications.md](../../docs/ft/daemon/telegram-notifications.md).
