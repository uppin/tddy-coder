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
    pub source: SessionNotificationSource, // ActivityStatus | AgentToolCall | Presenter | Agent | PendingQuestion | SessionEnded
    pub text: String,        // operator-facing line, rendered once at publish
    pub agent: Option<AgentNotification>, // what the agent wrote; Some only for source Agent
    pub ended: Option<SessionEndNotice>,  // how the agent ended; Some only for source SessionEnded
    pub activity_status: Option<String>,   // the status raised for; Some only for source ActivityStatus
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

`ended` carries how a session's agent ended (`SessionEndNotice { outcome, exit_code }`) for a
`SessionEnded` notification, raised by `tddy-session-lifecycle`'s agent-end forwarder when the
session manager announces the end (`#activity-wake` 1/3). The project notification log stores it as
`source: "session_ended"` with an `ended` object (omitted when `None`), so the project-home inbox
tools return it; Telegram declines the source and the drawer's dot ignores it.

`text` is rendered at publish rather than by each subscriber, so a chat message and the drawer's
tooltip read the same sentence. `os_user` is what scopes the stream — see **Authorization**.

## Naming

The label is resolved on the daemon's side of the boundary (`tddy-session-lifecycle`'s
`session_notifications` module, which keeps `SessionNotificationPublishing`), because `workflow_goal` comes from the
session-list enrichment that serves `ListSessions`. This crate receives the resolved label through
[`SessionLabels`](../src/service/ports.rs), which is the seam.

`tddy_core::session_label::session_display_label(display_name, repo_path, workflow_goal, session_id)`
is the one rule: the name the session was given (`display_name`, trimmed; blank counts as none),
else the basename of `repo_path`, else `workflow_goal`, else the first eight characters of the id.
It mirrors `packages/tddy-session-web/src/utils/sessionDrawerLabel.ts` case for case, and the em-dash
display placeholder counts as absent on both sides.

`resolve_session_label(agents, sessions_base, session_id)` performs the lookup from **the same four values
`ListSessions` reports** — anything else would agree with the drawer only until the two sources
diverged. It reads `display_name` and `repo_path` from the one `.session.yaml` parse, returns the name
when there is one, asks the worktree next, and pays for the goal lookup only when the rule will
consult it: this runs on every reported hook, twice per agent tool call.

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

**`ProjectNotificationLog`** (`project_notification_log`, `#home-inbox` 7/8) wants
`AttentionRequired` and appends it to the log of the session's project, which
`WaitProjectNotifications` and `ReadProjectNotifications` read. `tddy-daemon`'s `mount_host::open_shared`
opens it over the data dir before the bus is assembled and installs the same `Arc` on the session host
(`DaemonSessionHost::with_project_notification_log`).

- **Concurrency.** One `Mutex<ProjectState>` per project is held across the append (and a compaction),
  so the file's lines are in `seq` order and a record is in the window only once it is on disk. A
  per-project `tokio::sync::Notify` is woken with `notify_waiters` after the lock is released; a wait
  creates and `enable`s its `Notified` before it reads the window, so a record appended in between
  always wakes it.
- **Cursor of an empty page** is the newest `seq`, a deliberate divergence from the follow tools'
  "cursor unchanged": an unchanged empty cursor would let a wait "from now" skip what was published
  between two polls.
- **Storage** (`project_notification_log/storage.rs`): one serde DTO is both the file line and the
  tool's answer object. Compaction at twice the window writes a sibling `.jsonl.tmp`, `sync_all`s it,
  renames it over the file and syncs the directory. A torn last line (a crash mid-append) is ended
  with a line break on replay, so the next append does not run on into it.
- **Orchestrator.** `deliver` reads the session's changeset beside its record and stores its
  `orchestrator_session_id` on the `ProjectNotificationRecord` (the DTO omits it when `None`, and a
  line written without it replays as `None`). An unreadable changeset names no orchestrator; the
  record is still kept. `ProjectNotificationScope.started_by: Option<String>` keeps only the records
  naming that orchestrator (`started_by_me`).
- **Errors.** A session of no project is skipped with `Ok`; an unresolvable OS user or an unreadable
  record is an `Err` the bus logs. A failed compaction is logged and retried on the next append.
- **Dependencies.** The paging (`FollowPage`, `page_size`, `parse_cursor`) and `MAX_WAIT_MS` are
  `tddy-tool-engine`'s; the edge is acyclic, since tool-engine's tree holds no session-activity.
- **Activity status.** `notification_for_activity_status` sets `activity_status` to the wire status
  it was raised for; the log's DTO keeps it (`"activity_status"`, omitted when `None`, and a line
  written before records carried it reads back with none), so a finished turn and a wait for input
  stay told apart in a record.

## Session watches (`session_watches`)

An orchestrator's watches over the sessions it can see (`#activity-wake` 2/3): a filter — sessions
by id and/or `started_by_me`, six kinds (`WatchKind`: `question`, `permission`, `turn_finished`,
`waiting_for_input`, `agent_notification`, `ended`) — with its own position in the project
notification log and a required TTL of at most 24 hours (`MAX_WATCH_TTL_SECS` = 86 400, at most
`MAX_LIVE_WATCHES` = 16 live per orchestrator).

- **Matching.** `WatchKind::of(record)`: a `pending_question` with a permission question is
  `permission`, any other `pending_question` and a `presenter` record are `question`, an
  `activity_status` of `Done` is `turn_finished` and of `WaitingForInput` with no question is
  `waiting_for_input` (an activity-status record carrying a question matches nothing — its
  `pending_question` record already announced that prompt), an `agent` record is
  `agent_notification` and a `session_ended` record is `ended`. A record is selected when its
  session is named, or `started_by_me` and its `orchestrator_session_id` is the owner — always
  within the owner's project and OS user (the log's `records_after(project, os_user, seq)`, which
  excludes no session, so a watch can read its own orchestrator's end), and never the owner's own
  session.
- **Delivery.** `SessionWatchStore::take_delivery(owner, log, now)` ends what has ended, then reads
  each live watch's records after its position, unions the matches by `seq`, takes the oldest 50
  (`MAX_EVENTS_PER_ANSWER`), advances each watch's position past what it delivered, adds evictions
  to `lost`, and persists — one async lock per project serialises every operation, which is what
  makes delivery exactly-once across concurrent calls. Nothing waiting is an empty delivery and no
  write. The daemon attaches a non-empty delivery to the orchestrator's tool answers as
  `watchEvents` (`tddy-daemon-rpc`); the store only hands it over.
- **Endings**, checked at every operation on the owner's watches: `now ≥ expires_at` is `expired`;
  the owner's own `session_ended` record after the watch's creation is `orchestrator_ended` (read
  through `records_after`, unscoped by session); every `session_ids` entry having a `session_ended`
  record after creation, with `started_by_me` off, is `sessions_ended`. An ended watch is removed
  and reported once in the next delivery's `ended`.
- **Persistence.** Per project at `<data dir>/projects/<project_id>/session-watches.json`, the
  whole file atomically rewritten (write, sync, rename, sync the directory) on every change and
  read once at open; an unreadable file is an error. Keyed by the orchestrator's session id, which
  `/clear` keeps.
- `deliver_with(owner, log, now, deliver)` computes the same delivery as `take_delivery` but
  advances positions and removes ended watches only when `deliver` succeeds; on failure nothing
  moves and nothing is persisted, so a wake whose paste failed delivers nothing and what was
  waiting waits on. Nothing waiting never calls `deliver`.
- **Streaming to a monitor.** `SessionWatchStore::take_stream_batch(owner, stream_id, watch_ids,
  ack_seq, log, now)` (`#activity-wake` 4/5) hands an orchestrator's watch events to a process —
  `tddy-tools monitor` — instead of to tool answers or a wake. A `stream_id` of `None` starts a
  stream; the batch carries the id every later call presents. Under the project lock it first
  advances each named watch's position to `ack_seq` (refused, `StreamRefusal`, when `ack_seq` is
  past what was handed or names a watch the stream does not hold), renews the stream's lease, and
  answers the matched records after the new position (union by `seq`, oldest first, at most 50),
  the `lost` count and the ended watches — without advancing past them, so a batch the caller
  received but did not acknowledge is handed again. A new stream of the watch's own orchestrator
  takes the watch over at once and resumes from the last acknowledged position; the superseded
  stream is refused (`Superseded`) from its next call. A watch counts as streamed while a lease is
  held: the lease is in memory only, renewed by each call and lapsed `STREAM_LEASE_GRACE` (15 s;
  `with_stream_lease_grace` sets it) after the last one. Both `take_delivery` and `deliver_with`
  skip a streamed watch entirely, so while it streams — and for the grace after each call — its
  events reach the monitor alone and return to answers and the wake when the lease lapses. The
  lease map and the acknowledgement live in `session_watches/stream.rs`.
- `has_live_watches(owner, log, now)` is the live check the wake and the auto-stop hold call;
  this crate only answers it.

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
| `project_notification_log_acceptance` | `tddy-session-activity` | a wait returned by a record published after it began; timeouts and the 20 s cap; cursors across a restart; scoping by project, OS user and own session; `lost`; a question's id and content |
| `project_notification_log_unit` | `tddy-session-activity` | `wants`, cursor parsing, the empty-page cursor, paging and clamping, `lost`, skipping a session of no project, an unknown session's delivery error |
| `project_notification_log_storage` | `tddy-session-activity` | compaction past twice the window keeping `seq` and `lost` across a restart; a damaged line skipped; a torn last line not swallowing the next record |
| `project_home_wait_acceptance` | `tddy-daemon-rpc` | the tool through the exec-tool handler: a home agent receiving a child's notification, a non-home caller refused |
| `session_watches_unit` | `tddy-session-activity` | each kind matching only its records; one record delivered once for two watches and across concurrent calls; the 50 cap; `lost`; expiry and orchestrator-ended endings; reopening the store; a new watch seeing only what follows it |
| `project_notification_log_storage` (status cases) | `tddy-session-activity` | a status record round-tripping its `activity_status`; an older line without one still reading |
| `project_home_watch_tools` | `tddy-tool-engine` | the tools offered to the project-home role; `ttl_seconds` required in the schema; the port handed what was asked, with every kind by default; bad arguments refused before the port; renew and cancel naming the watch |
| `project_home_session_watch_acceptance` | `tddy-daemon-rpc` | the tools and `watchEvents` through the harness: a permission prompt riding on the next answer, delivered once; kinds and sessions told apart; list, renew and cancel; sessions-ended reported once; every refusal; an error answer consuming nothing |
| `session_watch_stream_unit` | `tddy-session-activity` | a stream batch not advancing until acknowledged; `ack_seq` past what was handed refused; `take_delivery` and `deliver_with` skipping a streamed watch; the lease lapsing after its grace; a new stream taking over from the old one |

⚠ The bus's classification, fan-out and stream are asserted from `tddy-session-lifecycle` and
`tddy-telegram-control`, not from this crate. Recorded in
[`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../../../docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).

## Related

- **[activity-service.md](./activity-service.md)** — the eight RPCs this crate serves, two of which publish here.
- **[telegram-notifier.md](../../tddy-telegram/docs/telegram-notifier.md)** — the surface this path was extracted from.
- **[daemon-kernel.md § Ports](../../tddy-daemon-kernel/docs/daemon-kernel.md#ports)** — the operator-channel port and the `awaits_operator` rule.
- **[connection-service.md](../../tddy-daemon/docs/connection-service.md)** — the 33 methods that stayed.
- **[../../../docs/ft/daemon/session-notifications.md](../../../docs/ft/daemon/session-notifications.md)** — product reference.
