# Session notifications (`tddy_session_activity::session_notifications`, `::session_notification_subscribers`)

## Overview

A **session notification** is "something happened in a session that an operator should know about".
The daemon publishes them onto a bus; subscribers declare which ones they want. Telegram is one
subscriber; the `StreamSessionNotifications` feed that drives `tddy-web`'s drawer indicators is
another.

The bus, the event, the subscriber trait and the three builders moved here from `tddy-daemon` with
`#unbundle` node 7, along with the `activity.ActivityService` methods that publish onto them. Two
fragments stayed in the daemon and are re-exported from it under the same module names, so no call
site there changed: a session's **display label**, which is read from `session_list_enrichment`
serving `ListSessions` (family C, which stays), and the **Telegram subscriber**, which delivers
through the daemon's bot hooks and reads its `telegram:` config block.

Before this existed, `telegram_notifier` *was* the notification system — one method classified the
event, rendered the copy, resolved the recipients and sent — so a second consumer could not be added
without reaching into it.

## The event

```rust
pub struct SessionNotification {
    pub session_id: String,
    pub label: String,       // the drawer label; see "Naming" below
    pub kind: SessionNotificationKind,     // Activity | AttentionRequired
    pub source: SessionNotificationSource, // ActivityStatus | AgentToolCall | Presenter
    pub text: String,        // operator-facing line, rendered once at publish
    pub at_unix_ms: u64,
    pub os_user: String,     // owner; server-side only, never on the wire
}
```

`text` is rendered at publish rather than by each subscriber, so a chat message and the drawer's
tooltip read the same sentence. `os_user` is what scopes the stream — see **Authorization**.

## Naming

The label is resolved on the daemon's side of the boundary (`tddy-daemon`'s `session_notifications`
module, which keeps `SessionNotificationPublishing`), because `workflow_goal` comes from the
session-list enrichment that serves `ListSessions`. This crate receives the resolved label through
[`SessionLabels`](../src/service.rs), which is the seam.

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
| `Presenter` | a `ModeChanged` requiring a human gate | `AttentionRequired` |
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

**`TelegramNotificationSubscriber`** — in `tddy-daemon`, implementing this crate's trait from the
far side of the crate boundary, which is what the trait is for — takes `AttentionRequired` **from
the activity-status path only**. It declines `Activity` (that kind exists for indicators; sending it would turn every tool
call into a message) and declines `Presenter` (those elicitations already reach a chat through
`telegram_notifier`, keyboards and per-chat FIFO included — taking them here would double-send).
Recipients are tracked-first, falling back to the configured broadcast list; when the tracking map
cannot be read at all it sends **nothing** rather than broadcasting.

**`SessionNotificationStreamSubscriber`** wants everything and pushes onto a `broadcast` channel
that `StreamSessionNotifications` subscribes to. Live-only: a replayed backlog would raise
indicators for turns that finished while the tab was closed.

## Publish sites

| Site | Source |
|---|---|
| `tddy_session_activity::service::ActivityServiceImpl::report_session_status` | `ActivityStatus` |
| `tddy_session_activity::service::ActivityServiceImpl::report_agent_activity` | `AgentToolCall` |
| `tddy_daemon::telegram_session_subscriber::run_presenter_observer_loop` | `Presenter` |

The first two are `activity.ActivityService` methods. Before `#unbundle` node 7 they were
`connection.ConnectionService`'s, and the hook clients that call them were re-pointed in the same
PR.

The presenter observer takes its two sinks — Telegram and the bus — **independently**, and is
spawned when either exists. Gating it on Telegram left workflow-session indicators dead on every
daemon without a `telegram:` block.

## Authorization

The bus is **host-wide**, so the relay is the only thing between one operator and another's
sessions. `stream_session_notifications` resolves `os_user_for_github` — through
[`OsUserResolver`](../src/service.rs), a port, because only the daemon holds the `users[]` mapping —
exactly as `stream_session_activity` does (`permission_denied` when unmapped), and
`relay_session_notifications` delivers only on a **positive** owner match — a notification whose
owner is empty reaches nobody.

## Known limitation

A **Telegram-started** workflow session does not publish presenter events: threading the bus onto
`TelegramWorkflowSpawn` touches five inbound-control harnesses that construct it by hand. Its
Telegram surface is unaffected, and web-started and resumed sessions publish normally. Tracked in
`docs/dev/TODO.md`.

## Tests

All of them are in **`packages/tddy-daemon/tests/`** and stayed there when the modules moved: each
is pinned by `ConnectionServiceImpl` or `test_util::{test_service, TEST_TOKEN}`, and moving either
would put `tddy-daemon` back on this crate's dependency path and defeat the extraction. They pass
where they are, and they drive the same re-exports production does.

`session_notification_bus_unit` (classification table, fan-out, `wants` filtering, failure
isolation), `telegram_notification_subscriber_unit` (interest filter, tracked-first routing,
dedupe), `session_notification_presenter_unit`, `session_notification_label_unit`,
`session_notifications_acceptance` (the real `ReportSessionStatus` RPC, including that neither the
hook token nor the bot token reaches a notification), `session_notifications_stream_acceptance`
(the RPC, its per-user scoping, and one subscription serving every session).

⚠ **`tddy-session-activity` itself has no tests at all** — not a `tests/` directory, not a `#[cfg(test)]`
module, across 1,573 production lines. Every assertion about this subsystem is made from
`tddy-daemon`. Recorded in
[`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../../../docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).

## Related

- **[activity-service.md](./activity-service.md)** — the eight RPCs this crate serves, two of which publish here.
- **[telegram-notifier.md](../../tddy-telegram/docs/telegram-notifier.md)** — the surface this path was extracted from.
- **[connection-service.md](../../tddy-daemon/docs/connection-service.md)** — the 33 methods that stayed.
- **[../../../docs/ft/daemon/session-notifications.md](../../../docs/ft/daemon/session-notifications.md)** — product reference.
