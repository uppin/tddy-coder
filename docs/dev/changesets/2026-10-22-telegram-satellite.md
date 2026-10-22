# 2026-10-22 — Telegram as a satellite: the operator channel port, the wiring edge, the status board

**Type:** Architecture

Telegram got the LiveKit treatment. The daemon's core defines one port, the **operator channel**,
through which a session's elicitations and attention notifications reach an operator's chat. The
Telegram crates implement it, and only `tddy-daemon` (the wiring) brings the two together.
`tddy-session-activity` and `tddy-session-lifecycle` stopped depending on `tddy-telegram`, which took
`teloxide` out of the library crates built on them, and a ratchet keeps it out. Telegram also reports
its state on the integration status board, and the daemon settings screen shows it.

## What changed

- **`tddy-daemon-kernel`.** Added `operator_channel`: `OperatorChannel` (`id`,
  `follows_presenter_events`, `on_presenter_event`, `wants_notification`, `on_notification`),
  `SharedOperatorChannel`, the null object `NoOperatorChannel`, and `awaits_operator`, the one rule
  for "this presenter mode waits on a human". Moved `SessionNotification`, `SessionNotificationKind`
  and `SessionNotificationSource` into `session_notification` so the port can name them. Removed
  `presenter_observer` (`PresenterEventSink`, `SharedPresenterEventSink`); its callers moved to the
  port. The port is in the kernel rather than in `tddy-daemon-extensions` because it carries
  `tddy-service`'s `ServerMessage`
  ([daemon-kernel.md § Ports](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md#ports)).
- **`tddy-session-activity`.** Dropped `tddy-telegram`. `session_notifications` re-exports the moved
  types and gained `OperatorChannelSubscriber`, the bus adapter for a channel. The presenter observer
  delivers to a `SharedOperatorChannel` and is spawned when the channel follows presenter events or a
  bus is present. The `pending_elicitation` list flag is read by the crate's own
  `pending_elicitation` module
  ([session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md),
  [presenter-observer.md](../../../packages/tddy-session-activity/docs/presenter-observer.md)).
- **`tddy-session-lifecycle`.** Dropped `tddy-telegram` and its four re-exports (`active_elicitation`,
  `elicitation`, `telegram_github_link`, `telegram_tracked_session`). `DaemonSessionHost` holds a
  `SharedOperatorChannel`
  ([session-service.md § The presenter observer](../../../packages/tddy-session-lifecycle/docs/session-service.md#the-presenter-observer)).
- **`tddy-telegram`.** Added `status`: `TELEGRAM_INTEGRATION_ID`, `configured_state`,
  `ReportingSender`, `BotIdentity` / `TeloxideBotIdentity`, `report_bot_identity` and
  `redact_bot_token`. No error written to the board carries the bot token. `elicitation` lost
  `mode_changed_requires_telegram_elicitation` and `pending_elicitation_for_session_dir`
  ([telegram-notifier.md § Integration status](../../../packages/tddy-telegram/docs/telegram-notifier.md#integration-status)).
- **`tddy-telegram-control`.** `TelegramDaemonHooks` (`TelegramDaemonHooks::new`) implements
  `OperatorChannel`. The separate `TelegramNotificationSubscriber` (`telegram_notification_subscriber.rs`)
  was folded into it, its per-session dedupe included, and the file deleted. `spawn_telegram_workflow`
  moved from `pickers.rs` to `telegram_session_control/workflow_launch.rs` and was split
  ([architecture.md § Telegram's operator channel](../../../packages/tddy-telegram-control/docs/architecture.md#telegrams-operator-channel)).
- **`tddy-daemon`.** Names `tddy-telegram` directly. `runtime/mount_host.rs` installs the hooks, or
  `NoOperatorChannel`, on the session host and on the bus. `runtime/integrations.rs` reports
  Telegram's configured state. `runtime/telegram_wiring.rs` wraps the sender in `ReportingSender` and
  builds the identity check. `runtime/tasks.rs` runs the check before the dispatcher
  ([daemon-endpoint.md](../../../packages/tddy-daemon/docs/daemon-endpoint.md#integration-status-the-livekit-plugin-and-the-telegram-plugin)).
- **`tddy-daemon-extensions`.** Added the ratchet `tests/telegram_is_a_satellite.rs`, beside LiveKit's
  ([extension-ports.md](../../../packages/tddy-daemon-extensions/docs/extension-ports.md#telegram-the-operator-channel-and-its-ratchet)).
- **`tddy-web`.** The settings screen shows Telegram's status line, `daemon-settings-telegram-status`
  ([daemon-settings-screen.md](../../../packages/tddy-web/docs/daemon-settings-screen.md)).

Product view: [daemon settings § Integration status](../../ft/daemon/daemon-settings.md#integration-status),
[telegram notifications](../../ft/daemon/telegram-notifications.md),
[session notifications](../../ft/daemon/session-notifications.md).

## Measurements

**Dependency closure.** Workspace crates that can be built with a Telegram crate (`cargo metadata`,
normal and build edges): **11 before** (`tddy-agent-launch`, `tddy-cli-sessions`, `tddy-daemon`,
`tddy-daemon-livekit`, `tddy-daemon-rpc`, `tddy-demo-vm-service`, `tddy-desktop`,
`tddy-session-activity`, `tddy-session-lifecycle`, `tddy-session-split`, `tddy-telegram-control`).
**After:** only the Telegram crates themselves and the crates that build the daemon (`tddy-daemon`,
`tddy-desktop`). Held by `telegram_is_a_satellite`.

**File-length gate** (production lines to the first `#[cfg(test)]`; every other changed source file
is under 500):

| File | master | green | wrapped |
|---|---:|---:|---:|
| `tddy-telegram-control/src/telegram_notifier.rs` | 1,246 | 1,256 | **1,245** |
| `tddy-telegram-control/src/telegram_session_control/pickers.rs` | 706 | 706 | **617** (still over 500) |

**Code issues, reconciled.**

- `tddy-telegram-control` `pickers.rs` `spawn_telegram_workflow` — a record added and closed in this
  change: CRAP **306** (complexity 17, untested) → **< 9** (not in the report's top 50; covered by
  `tests/telegram_workflow_launch_acceptance.rs`). Moved to `workflow_launch.rs` and split into
  `workflow_spawn_deps`, `project_for_workflow_spawn`, `merge_chain_child_integration_base`,
  `explicit_integration_base_ref`, `follow_spawned_workflow` and `announce_started_workflow`. The new
  suite also covers `handle_telegram_agent_callback` (CRAP 30 → covered). Record deleted.
- `tddy-session-activity` `presenter_observer_task.rs` `run_presenter_observer_loop` — CRAP **90**
  (complexity 9, untested) → **6** (complexity 2), over `follow_presenter_events` (CRAP 5) and
  `deliver_presenter_event` (CRAP 4), both unit-tested. No record left.
- Narrowed or re-measured, kept: `oversized-file-telegram-session-control-pickers.md` (706 → 617);
  `oversized-file-telegram-notifier.md` (1,246 → 1,245);
  `complexity-telegram-notifier-on-server-message.md` (214 → 213 lines, CRAP 31, covered);
  `tddy-session-lifecycle`'s `oversized-file-connection-service.md` (±0).
- `tddy-tools analyze` joins: `tddy-session-activity` 76.9% → 77.0%, `tddy-telegram-control`
  62.7% → 63.3%, `tddy-daemon` 70.6% → 71.2%.

## Backlog

- "Session notifications — three follow-ups left open" (`2026-08-29-session-notifications-three-follow-ups-left-open`),
  bullet 1 — **partly answered, kept**. Elicitations still bypass the bus, but both of a session's
  paths to its operator end at the `OperatorChannel` port, which is where a fold onto the bus would
  land.
- Added: "`move_module_to_crate` with `reexport: none` re-points an existing glob facade, closing a
  cycle" (`2026-10-22-restructure-crate-move-reexport-none-repoints-an-existing-glob-facade-into-a-cycle`),
  a restructure-engine defect met during the green phase.
