# Session notifications (`tddy_session_activity::session_notifications`, `::session_notification_subscribers`)

## Overview

A **session notification** is "something happened in a session that an operator should know about".
The daemon publishes them onto a bus; subscribers declare which ones they want. The daemon's
**operator channel** (Telegram, when one is configured) is one subscriber; the
`StreamSessionNotifications` feed that drives `tddy-web`'s drawer indicators is another.

The bus, the subscriber trait and the three builders moved here from `tddy-daemon` with `#unbundle`
node 7, along with the `activity.ActivityService` methods that publish onto them. A session's
**display label** stayed on the daemon's side, read from `session_list_enrichment` serving
`ListSessions` (family C), and `tddy-session-lifecycle` re-exports this module under the same name.

The event types — `SessionNotification`, `SessionNotificationKind`, `SessionNotificationSource` — are
defined in `tddy_daemon_kernel::session_notification`, because the kernel's operator-channel port names
them, and re-exported from `session_notifications` under their usual paths. This crate names no
Telegram crate.

Before this existed, `telegram_notifier` *was* the notification system — one method classified the
event, rendered the copy, resolved the recipients and sent — so a second consumer could not be added
without reaching into it.

## The event

```rust
pub struct SessionNotification {
    pub session_id: String,
    pub label: String,       // the drawer label; see "Naming" below
    pub kind: SessionNotificationKind,     // Activity | AttentionRequired
    pub source: SessionNotificationSource, // ActivityStatus | AgentToolCall | Presenter | Agent
    pub text: String,        // operator-facing line, rendered once at publish
    pub agent: Option<AgentNotification>, // what the agent wrote; Some only for source Agent
    pub at_unix_ms: u64,
    pub os_user: String,     // owner; server-side only, never on the wire
}
```

`agent` carries an agent-written notification's structure — `title`, `body_markdown`, `level`
(`NotificationLevel`: `Info` | `Success` | `Warning` | `Error`), `sound` and the session's web
`link` (`#/sessions/<id>?host=<ActivityPorts::daemon_instance_id>`) — beside a `text` that is the title
alone, because each surface renders the body differently and none can recover structure from a
flattened line. `streams::session_notification_event` maps it onto `SessionNotificationEvent`'s
`title`, `body_markdown`, `level`, `sound` and `link` (empty for every other source).

`text` is rendered at publish rather than by each subscriber, so a chat message and the drawer's
tooltip read the same sentence. `os_user` is what scopes the stream — see **Authorization**.

## Naming

The label is resolved on the daemon's side of the boundary (`tddy-session-lifecycle`'s
`session_notifications` module, which keeps `SessionNotificationPublishing`), because `workflow_goal` comes from the
session-list enrichment that serves `ListSessions`. This crate receives the resolved label through
[`SessionLabels`](../src/service/ports.rs), which is the seam.

`tddy_core::session_label::session_display_label(repo_path, workflow_goal, session_id)` is the one
rule: the basename of `repo_path`, else `workflow_goal`, else the first eight characters of the id.
It mirrors `packages/tddy-web/src/utils/sessionDrawerLabel.ts` case for case, and the em-dash
display placeholder counts as absent on both sides.

`resolve_session_label(sessions_base, session_id)` performs the lookup from **the same three values
`ListSessions` reports** — anything else would agree with the drawer only until the two sources
diverged. It asks the worktree first and pays for the goal lookup only when the rule will consult
it: this runs on every reported hook, twice per agent tool call.

## Classification

| Source | Input | Kind |
|---|---|---|
| `ActivityStatus` | `WaitingForInput`, `Done` | `AttentionRequired` |
| `ActivityStatus` | `Started`, `Running`, `ExecutingTool` | `Activity` |
| `ActivityStatus` | `Ended`, unknown, empty | *no notification* |
| `AgentToolCall` | any `ReportAgentActivity` tool call | `Activity` |
| `Presenter` | a `ModeChanged` that awaits the operator (`tddy_daemon_kernel::operator_channel::awaits_operator`) | `AttentionRequired` |
| `Presenter` | `StateChanged`, `GoalStarted`, `BackendSelected` | `Activity` |
| `Presenter` | an autonomous mode, or an event carrying none | *no notification* |

## The bus

`SessionNotificationBus::publish` offers the notification to each subscriber whose `wants` returns
true, in registration order, awaiting each. **A `deliver` error is logged and swallowed**: publishing
happens inside `ReportSessionStatus` / `ReportAgentActivity`, which must still return `ok`, and one
subscriber's failure must not starve the others.

Dedupe lives in the **subscriber**, not the bus. The bus publishes every reported status so an
indicator can stay alive; a chat must not receive the repeats.

## Subscribers

**`OperatorChannelSubscriber`** — the bus adapter for the kernel's
[`OperatorChannel`](../../tddy-daemon-kernel/docs/daemon-kernel.md#ports) port.
`OperatorChannelSubscriber::new(channel)` takes a `SharedOperatorChannel`; its `name()` is the
channel's `id()`, `wants` asks the channel's `wants_notification` and `deliver` calls its
`on_notification`. `tddy-daemon`'s `runtime/mount_host.rs` registers one on the daemon's bus with the
same channel it installs on the session host — Telegram's, or `NoOperatorChannel`, which wants
nothing.

Telegram's channel (`TelegramDaemonHooks`, in
[`tddy-telegram-control`](../../tddy-telegram-control/docs/architecture.md)) takes `AttentionRequired`
**from the activity-status path only**. It declines `Activity` (that kind exists for indicators;
sending it would turn every tool call into a message) and declines `Presenter` (those elicitations
already reach the channel through `on_presenter_event`, keyboards and per-chat FIFO included — taking
them here would double-send). It dedupes per session with `LastDeliveredPerSession`. Recipients are
tracked-first, falling back to the configured broadcast list; when the tracking map cannot be read at
all it sends **nothing** rather than broadcasting.

**`SessionNotificationStreamSubscriber`** wants everything and pushes onto a `broadcast` channel
that `StreamSessionNotifications` subscribes to. Live-only: a replayed backlog would raise
indicators for turns that finished while the tab was closed.

## Publish sites

| Site | Source |
|---|---|
| `tddy_session_activity::service::ActivityServiceImpl::report_session_status` | `ActivityStatus` |
| `tddy_session_activity::service::ActivityServiceImpl::report_agent_activity` | `AgentToolCall` |
| `tddy_session_activity::presenter_observer_task` (`deliver_presenter_event`) | `Presenter` |
| `tddy_session_activity::service::ActivityServiceImpl::report_operator_notification` | `Agent` |

The first two and the last are `activity.ActivityService` methods. Before `#unbundle` node 7 they were
`connection.ConnectionService`'s, and the hook clients that call them were re-pointed in the same
PR.

The presenter observer hands each event to the daemon's operator channel and then to the bus,
**independently**: it is spawned when the channel `follows_presenter_events()` or a bus is present.
`NoOperatorChannel` follows none, so on a daemon without a `telegram:` block the observer runs for the
bus alone, and a workflow session's indicator still lights.

## Authorization

The bus is **host-wide**, so the relay is the only thing between one operator and another's
sessions. `stream_session_notifications` resolves `os_user_for_github` — through
[`OsUserResolver`](../src/service/ports.rs), a port, because only the daemon holds the `users[]` mapping —
exactly as `stream_session_activity` does (`permission_denied` when unmapped), and
`relay_session_notifications` delivers only on a **positive** owner match — a notification whose
owner is empty reaches nobody.

## Known limitation

A **Telegram-started** workflow session does not publish presenter events: threading the bus onto
`TelegramWorkflowSpawn` touches five inbound-control harnesses that construct it by hand. Its
Telegram surface is unaffected, and web-started and resumed sessions publish normally. Marked
`TODO(session-notifications)` in `tddy-telegram-control`'s `telegram_session_control/workflow_launch.rs`.

## Tests

| Suite | Crate | Covers |
|---|---|---|
| `operator_channel_subscriber_acceptance` | `tddy-session-activity` | a channel on the bus receives what it wants and is not handed what it declines; the subscriber is named after its channel; a select prompt raises attention with no channel wired |
| `presenter_observer_task` (unit) | `tddy-session-activity` | the observer's run condition, per-event delivery to the channel and the bus, and a failing channel or read ending it |
| `session_notification_bus_unit` | `tddy-session-lifecycle` | classification table, fan-out, `wants` filtering, failure isolation |
| `session_notification_presenter_unit` | `tddy-session-lifecycle` | presenter-event classification |
| `session_notification_label_unit` | `tddy-session-lifecycle` | label resolution |
| `session_notifications_stream_acceptance` | `tddy-session-lifecycle` | the RPC, its per-user scoping, and one subscription serving every session |
| `telegram_notification_subscriber_unit` | `tddy-telegram-control` | the Telegram channel's interest filter, tracked-first routing, dedupe |
| `telegram_operator_channel_acceptance` | `tddy-telegram-control` | the Telegram channel's port contract: its id, what it wants and declines, delivery to the tracked chat, one send per unchanged alert |
| `session_notifications_acceptance` | `tddy-telegram-control` | the real `ReportSessionStatus` RPC reaching Telegram, including that neither the hook token nor the bot token reaches a notification |

⚠ The bus's classification, fan-out and stream are asserted from `tddy-session-lifecycle` and
`tddy-telegram-control`, not from this crate. Recorded in
[`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../../../docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).

## Related

- **[activity-service.md](./activity-service.md)** — the eight RPCs this crate serves, two of which publish here.
- **[telegram-notifier.md](../../tddy-telegram/docs/telegram-notifier.md)** — the surface this path was extracted from.
- **[daemon-kernel.md § Ports](../../tddy-daemon-kernel/docs/daemon-kernel.md#ports)** — the operator-channel port and the `awaits_operator` rule.
- **[connection-service.md](../../tddy-daemon/docs/connection-service.md)** — the 33 methods that stayed.
- **[../../../docs/ft/daemon/session-notifications.md](../../../docs/ft/daemon/session-notifications.md)** — product reference.
