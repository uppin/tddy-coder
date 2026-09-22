# tddy-telegram-control architecture

## Overview

The crate is the Telegram front-end of the daemon's sessions. Inbound, `telegram_bot` long-polls
the Bot API and hands each message and callback to `TelegramSessionControlHarness`, which writes
session directories (`changeset.yaml`), spawns sessions through `tddy-session-lifecycle`, and sends
presenter input over `PresenterIntent`. Outbound, `TelegramSessionWatcher` turns a session's
presenter events and metadata ticks into chat messages and keyboards.

It sits above both `tddy-telegram` and `tddy-session-lifecycle` and is reached by
`tddy-daemon` alone — see [README.md](../README.md) for why it is a crate of its own.

## `telegram_session_control/` — seven files, one harness

`mod.rs` holds `TelegramSessionControlHarness`, its DTOs and the inherent methods more than one flow
calls. Four child modules (`session_start`, `pickers`, `chaining_and_listing`, `elicitation`) each
add one `impl TelegramSessionControlHarness` block for one family of flows. A private method is
visible only inside the module that declares it and its children, so a helper two families share
lives in `mod.rs`, where every child can see it; a free helper they share is `pub(super)` in
`callbacks` or `workflow_spawn`. Those two hold no harness methods: `callbacks` is pure functions,
and `workflow_spawn` is `TelegramWorkflowSpawn` and its helpers.

| Module | Prod lines | Holds |
|---|---:|---|
| `callbacks.rs` | 736 | the command and `callback_data` vocabulary: the slash commands, the prefixes, and the pure parsers, formatters and chunkers (`parse_*`, `chunk_telegram_text`) with their unit tests |
| `session_start.rs` | 711 | `/start-workflow`, `/start-claude`, `/start-cursor`: session creation, the model-pick callbacks, and the Claude and Cursor CLI spawns |
| `pickers.rs` | 705 | the project, branch-intent, branch, branch-conflict, model and agent keyboards, ending in `spawn_telegram_workflow` |
| `mod.rs` | 539 | the harness, its DTOs, the shared helpers |
| `chaining_and_listing.rs` | 518 | `/chain-workflow` and its parent picker, recipe and plan-review callbacks, the session list with Enter and Delete |
| `elicitation.rs` | 456 | presenter input from Telegram: feature submission, document review, elicitation answers, the changeset writes and recipe keyboard around them |
| `workflow_spawn.rs` | 375 | `TelegramWorkflowSpawn`: the chain integration-base merge, the spawn configuration and inputs, and the branch-name helpers the start paths share |

Production lines are counted to the first `#[cfg(test)]`. No module may exceed **800**, which
`telegram_extraction_shape`'s `no_control_module_exceeds_eight_hundred_production_lines` enforces.
The repo's file budget is 500. The five modules over it each have a record in
[code-issues/](code-issues/) naming its seams. Three of them (`session_start.rs`, `pickers.rs` and
the production half of `callbacks.rs`) cannot reach 500 by moving items alone, because their size
is in long functions: two CLI spawns of 217 and 137 lines, and the 177-line
`handle_telegram_branch_callback`.

## Telegram's two sinks

A workflow session's presenter events reach Telegram through a port, and its activity-status
events through the notification bus. The crate provides one implementation of each, and
`tddy-daemon`'s `runtime.rs` wires both when the daemon has a `telegram:` block.

### The presenter-event sink

`TelegramDaemonHooks` (`telegram_session_subscriber`) holds the daemon's `DaemonConfig`, the shared
`TelegramSender` and the `TelegramSessionWatcher`. It implements
`tddy_daemon_kernel::presenter_observer::PresenterEventSink`: each event locks the watcher and calls
`on_server_message`. An error ends that session's observer loop.

`runtime.rs` hands the hooks to `DaemonSessionHost::new` as `Option<SharedPresenterEventSink>`. The
observer loop itself — connect with retry, read the stream, publish to the notification bus — is
`tddy_session_lifecycle::presenter_observer_task::spawn_presenter_observer_task`. It runs whether
or not Telegram is configured, because the bus half lights the drawer indicator in `tddy-web` on
every daemon. See [`daemon-kernel.md`](../../tddy-daemon-kernel/docs/daemon-kernel.md) § Ports.

The Telegram-started spawn path (`pickers.rs`, `spawn_telegram_workflow`) starts the same loop with
the hooks as its sink and **no** bus. A Telegram-started workflow session therefore raises no drawer
indicator. That is recorded as a `TODO(session-notifications)` at the call site: publishing needs
the bus and the sessions base on `TelegramWorkflowSpawn`, which every inbound-control harness
constructs by hand.

### The notification subscriber

`TelegramNotificationSubscriber` (`telegram_notification_subscriber`) is one subscriber on
`tddy-session-activity`'s `SessionNotificationBus`. It takes `AttentionRequired` from the
activity-status path, declines `Activity` and `Presenter`, and sends tracked-first — see
[session-notifications.md](../../tddy-session-activity/docs/session-notifications.md).

`runtime.rs` builds the daemon's bus with this subscriber beside the `StreamSessionNotifications`
relay and installs it with `with_session_notification_bus`. `DaemonSessionHost::new` installs no
bus of its own: a host holding only the port cannot name this subscriber. A test that wants the
subscriber on a host installs the bus the way `runtime.rs` does
(`telegram_claude_cli_activity_alert_acceptance`).

## Log targets

Log targets keep their `tddy_daemon::` names: `tddy_daemon::telegram`,
`tddy_daemon::telegram_bot`, `tddy_daemon::telegram_session_control`,
`tddy_daemon::telegram_multi_select_shortcuts` and `tddy_daemon::session_notifications`. An
operator's `log:` filter in `daemon.yaml` is an observable interface, so the crate a line comes from
does not show in its target.

## Tests

The 15 suites in `tests/` exercise the crate through its own paths
(`tddy_telegram_control::…`). The Claude and Cursor start suites wait on a PTY with
`tddy_testing_commons::wait::a_capture_showing` and its `PTY_STUB_OUTPUT` ceiling, shared with
`tddy-session-lifecycle`'s CLI suites.

The crate's shape — the port, the manifests, the module budget — is pinned from
`packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs`, beside the port it asserts on.
