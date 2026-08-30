# Changeset: Session notifications as indicators

**Created:** 2026-08-29
**Status:** Green (all tests passing)
**PRD:** docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md

## Affected Packages

- [ ] `tddy-core` — new `session_label` module: the one shared session-display-label rule.
- [ ] `tddy-service` — `connection.proto`: `StreamSessionNotifications` RPC, event message, kind
      and source enums.
- [ ] `tddy-daemon` — new `session_notifications` module (bus, subscriber trait, classifier);
      Telegram becomes a subscriber; publish sites in `connection_service` and
      `telegram_session_subscriber`; new stream handler + tonic adapter arm.
- [ ] `tddy-web` — notification stream hook, per-session notification registry, pure indicator
      derivation, drawer dot states + blink animation, clear-on-select.

## State A (Current)

**Session naming is invented three times.** `session_telegram_label`
(`telegram_notifier.rs:51`) → first two UUID segments. `sessionDrawerLabel`
(`tddy-web/src/utils/sessionDrawerLabel.ts:11`) → repo basename → workflow goal → id[0:8].
`pr_stack/mod.rs:1534` → `Changeset.name` else branch. No shared Rust helper exists.

**Notification = Telegram.** `TelegramSessionWatcher::on_claude_cli_activity_status_changed`
(`telegram_notifier.rs:421`) classifies the status, dedupes on `last_activity_status`, resolves
recipients tracked-first, renders the copy and sends — one method, one consumer. It is called
inline from `connection_service::report_session_status` (`connection_service.rs:12956`).
`TelegramDaemonHooks` (`telegram_session_subscriber.rs:19`) is the only injection seam and it
is Telegram-shaped: `{ config, sender, watcher }`.

**The web's dot is poll-derived and focus-limited.** `connectionStatusForSession` maps
`is_active` + `pending_elicitation` to `connected | disconnected | needs-input`, rendered by
`SessionDrawerItem.tsx:67` (and duplicated in the collapsed strip, `SessionDrawer.tsx:262`).
`STATUS_COLOR` is declared twice (`SessionDrawerItem.tsx:30`, `SessionDrawer.tsx:17`). The only
live activity feed, `useAcpReplay`'s `COUNT_THEN_LIVE` stream, opens for the focused session
alone (`useAcpReplay.ts:99`). `useSessionActivity.ts` exists with `unreadCount`/`markSeen` and
has no consumer.

**Seen-state precedent.** `agentActivityRegistry` (`agentActivityRegistry.ts:238`) is a
module-level, per-session, in-memory store consumed via `useSyncExternalStore`, with a
`seenCount` baseline and `markSeen`. It is reset between component tests by
`cypress/support/component.ts:11`.

**Animation precedent.** `connectionTerminalChromeDotStyles.ts:2` — hand-written keyframes
injected as an inline `<style>`, with a `prefers-reduced-motion` guard. The only such
animation in the app.

## State B (Target)

`tddy_core::session_display_label(repo_path, workflow_goal, session_id)` is the single label
rule, mirroring `sessionDrawerLabel` exactly. The daemon resolves a session's label from the
same three values `ListSessions` reports, so a Telegram message and a drawer row agree by
construction.

`tddy_daemon::session_notifications` owns the notification domain:

```rust
pub enum SessionNotificationKind   { Activity, AttentionRequired }
pub enum SessionNotificationSource { ActivityStatus, AgentToolCall, Presenter }

pub struct SessionNotification {
    pub session_id: String,
    pub label: String,
    pub kind: SessionNotificationKind,
    pub source: SessionNotificationSource,
    /// Operator-facing line. Telegram sends it verbatim; the web shows it as the dot's tooltip.
    pub text: String,
    pub at_unix_ms: u64,
    /// The OS user whose sessions directory this session lives in. Server-side only — it is what
    /// scopes the notification stream to its subscriber, and never reaches the wire.
    pub os_user: String,
}

#[async_trait]
pub trait SessionNotificationSubscriber: Send + Sync {
    fn name(&self) -> &'static str;
    fn wants(&self, notification: &SessionNotification) -> bool;
    /// A delivery error is logged by the bus and never propagated: this runs inside an RPC that
    /// must still return `ok` (NFR3).
    async fn deliver(&self, notification: &SessionNotification) -> anyhow::Result<()>;
    /// The stream subscriber's hand-off to `StreamSessionNotifications`; `None` for every other
    /// subscriber.
    fn client_relay(&self) -> Option<broadcast::Receiver<SessionNotification>> { None }
}

pub struct SessionNotificationBus { /* Vec<Arc<dyn SessionNotificationSubscriber>> */ }
impl SessionNotificationBus {
    pub fn new() -> Self;
    /// Generic over the concrete subscriber so a caller can keep its own `Arc<S>` handle.
    pub fn with_subscriber<S: SessionNotificationSubscriber + 'static>(self, s: Arc<S>) -> Self;
    pub async fn publish(&self, notification: SessionNotification);
    pub fn subscribe_clients(&self) -> Option<broadcast::Receiver<SessionNotification>>;
}

/// `None` for a status that carries nothing an operator needs to see (`Ended`, unknown, empty).
pub fn notification_for_activity_status(...) -> Option<SessionNotification>;
pub fn notification_for_agent_tool_call(...) -> SessionNotification;
/// `None` for a presenter event that says nothing about working-or-waiting.
pub fn notification_for_presenter_event(...) -> Option<SessionNotification>;

/// The label lookup, from the same three values `ListSessions` reports.
pub fn resolve_session_label(sessions_base: &Path, session_id: &str) -> String;

/// What a publish site needs: the bus, plus the sessions base and owner a label is read from.
pub struct SessionNotificationPublishing { /* bus, sessions_base, os_user */ }

/// A recording double, shipped in `src/` on the same precedent as `InMemoryTelegramSender`.
pub struct RecordingSessionNotificationSubscriber;
```

Two subscribers ship:

- `TelegramNotificationSubscriber` — `wants` = `AttentionRequired` **and**
  `source == ActivityStatus`; `deliver` performs today's dedupe, tracked-first routing and send
  with today's copy. Telegram traffic is byte-identical to before, except the label.
- `SessionNotificationStreamSubscriber` — `wants` everything; `deliver` pushes onto a
  `broadcast::Sender<SessionNotification>` that `StreamSessionNotifications` subscribes to.

Publish sites: `report_session_status` (`connection_service.rs:12956`, replacing the inline
Telegram call), `report_agent_activity` (beside `agent_activity_hub.publish`,
`connection_service.rs:13086`), and `run_presenter_observer_loop`
(`telegram_session_subscriber.rs:86`, publishing alongside the existing `on_server_message`
call — see the PRD on why the presenter's keyboard path stays where it is).

`tddy-web` gains a `sessionNotificationRegistry` (mirroring `agentActivityRegistry`), a
`useSessionNotifications` hook opening one daemon-level stream, and a pure
`sessionIndicatorFor(session, state, nowMs)` returning
`disconnected | needs-input | working | connected`. The drawer dot renders that token in
`data-status` and adds the blink class for `working`. `handleSelectSession` marks seen.

## Delta

### New

- `packages/tddy-core/src/session_label.rs` — `session_display_label`.
- `packages/tddy-daemon/src/session_notifications.rs` — kinds, sources, event, subscriber trait,
  bus, classifiers.
- `packages/tddy-daemon/src/session_notification_subscribers.rs` — `TelegramNotificationSubscriber`,
  `SessionNotificationStreamSubscriber`.
- `packages/tddy-web/src/lib/sessionIndicator.ts` — `SessionIndicator`,
  `ACTIVITY_BLINK_WINDOW_MS`, `sessionIndicatorFor`.
- `packages/tddy-web/src/components/sessions/sessionNotificationRegistry.ts` — per-session
  `{ lastActivityAtMs, attentionAtMs, seenAtMs }` store.
- `packages/tddy-web/src/rpc/useSessionNotifications.ts` — the single daemon-level stream hook.
- `packages/tddy-web/src/components/sessions/sessionIndicatorDotStyles.ts` — blink keyframes +
  reduced-motion guard.

### Modified

- `packages/tddy-service/proto/connection.proto` — `StreamSessionNotifications` RPC,
  `StreamSessionNotificationsRequest`, `SessionNotificationEvent`, `SessionNotificationKind`,
  `SessionNotificationSource`.
- `packages/tddy-daemon/src/telegram_notifier.rs` — `on_claude_cli_activity_status_changed` becomes
  the Telegram subscriber's delivery body (see Removed); `chats_tracking_session` is extracted for
  the subscriber to route with. `session_telegram_label` and its 14 call sites are untouched.
- `packages/tddy-daemon/src/telegram_session_subscriber.rs` — carries the bus alongside the
  Telegram hooks; publishes presenter events.
- `packages/tddy-daemon/src/connection_service.rs` — bus field + accessor, publish at the two
  RPC sites, `stream_session_notifications` handler.
- `packages/tddy-daemon/src/connection_tonic_adapter.rs` — stream arm.
- `packages/tddy-daemon/src/main.rs` — assembles the bus with both subscribers.
- `packages/tddy-web/src/utils/connectionStatusForSession.ts` — **unchanged, and still live.** The
  plan expected it to become dead; it did not. `sessionIndicatorFor` supersedes it at the two drawer
  *dot* call sites only, and its remaining consumers ask a different question — "is this session
  dormant?" — with no notion of activity or attention: `sessionStackGroups.ts:113`
  (`partitionSessionsByActivity`, the Active/Remaining split) and `sessionBaseView.ts:18` (which base
  view a session renders). Both are deliberately blind to notifications: a row must not migrate
  between drawer partitions, nor a pane swap its base view, because an agent ran a tool.
- `packages/tddy-web/src/utils/sessionDrawerLabel.ts` — the em-dash **display placeholder** that
  `session_list_enrichment` puts in `workflow_goal` when a session has no changeset
  (`SessionListStatusDisplay::all_placeholders`) now counts as an absent goal, on both sides of the
  wire. Taking it literally would name a claude-cli session `—` — consistently on both surfaces,
  and uselessly on both. Small, deliberate amendment to the shared rule rather than a Telegram-only
  special case.
- `packages/tddy-web/src/components/sessions/SessionDrawerItem.tsx`,
  `SessionDrawer.tsx` — four-state dot, single shared `STATUS_COLOR`, blink class.
- `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx` — opens the stream,
  marks seen on select.
- `packages/tddy-web/cypress/support/component.ts` — reset the new registry between tests.
- `packages/tddy-web/cypress/support/rpc/connectionServiceBackend.ts` — a scriptable
  `StreamSessionNotifications` fake.

### Removed

- **`TelegramSessionWatcher::on_claude_cli_activity_status_changed`** (a `pub` method) and its
  `last_activity_status` field, along with the 7 in-file unit tests that covered them. Keeping the
  method would have left a second, unreachable copy of the notification copy and a second dedupe
  store — exactly the drift FR1/FR7 exist to remove. Every behaviour those tests pinned is re-pinned
  in `tests/telegram_notification_subscriber_unit.rs`, and the pre-existing
  `telegram_claude_cli_activity_alert_acceptance` suite still exercises the whole path through the
  real RPC, unmodified.
- **`TelegramSessionWatcher::chats_tracking_session` changed signature** to `Option<Vec<i64>>`: the
  routing that used to live inside the deleted method now needs to tell "nobody claimed this
  session" apart from "the tracking map could not be read", because only the first may fall back to
  broadcasting.

`session_telegram_label` is **not** removed and none of its call sites changed — the inbound
`/sessions` list and chain-parent buttons still key callback data off the short id. FR1 unifies the
label on the *notification* path only.

## Milestones

### Milestone 0: Planning
- [x] Create/update PRD documentation
- [x] Create changeset

### Milestone 1: One session label
- [x] `tddy_core::session_display_label` with the drawer's three-step rule
- [x] Daemon label resolver reading the same values `ListSessions` reports
- [x] Telegram activity alerts named by it

### Milestone 2: The notification bus
- [x] Kinds, sources, event, subscriber trait, bus
- [x] `notification_for_activity_status` / `notification_for_agent_tool_call` classifiers
- [x] `TelegramNotificationSubscriber` wrapping today's dedupe, routing and copy
- [x] `report_session_status` publishes instead of calling Telegram inline

### Milestone 3: The notification stream
- [x] Proto: RPC, request, event, enums
- [x] `SessionNotificationStreamSubscriber` + `stream_session_notifications` handler
- [x] Tonic adapter arm; bus assembled in `main.rs`
- [x] `report_agent_activity` and the presenter observer loop publish

### Milestone 4: Web indicators
- [x] `sessionIndicatorFor` + `ACTIVITY_BLINK_WINDOW_MS`
- [x] `sessionNotificationRegistry`
- [x] `useSessionNotifications` — one stream for the whole drawer
- [x] Four-state dot + fade-in/fade-out keyframes + reduced-motion guard
- [x] Mark seen on select

## Testing Strategy

### Acceptance Tests

**`packages/tddy-core/tests/session_display_label_acceptance.rs`** — FR1 / AC1, AC2. Mirrors
`packages/tddy-web/src/utils/sessionDrawerLabel.test.ts` case for case.

- [ ] `names_a_session_after_the_basename_of_its_repository_path`
- [ ] `ignores_a_trailing_slash_when_taking_the_repository_basename`
- [ ] `ignores_surrounding_whitespace_around_the_repository_path`
- [ ] `falls_back_to_the_workflow_goal_when_the_session_has_no_repository_path`
- [ ] `falls_back_to_the_workflow_goal_when_the_repository_path_is_the_filesystem_root`
- [ ] `falls_back_to_the_first_eight_characters_of_the_session_id_when_nothing_else_is_set`
- [ ] `falls_back_to_the_whole_session_id_when_it_is_shorter_than_eight_characters`

**`packages/tddy-daemon/tests/session_notifications_acceptance.rs`** — FR1/AC1, FR2/AC3, FR7/AC4.
Drives the real `ReportSessionStatus` RPC against a real session dir.

- [ ] `a_waiting_for_input_hook_names_the_session_after_its_repository_directory`
- [ ] `a_waiting_for_input_hook_reaches_the_indicator_subscriber_alongside_telegram`
- [ ] `an_executing_tool_hook_reaches_the_indicator_subscriber_but_sends_no_telegram_message`

**`packages/tddy-daemon/tests/session_notifications_stream_acceptance.rs`** — FR3/AC5, NFR1.

- [ ] `streams_an_attention_event_carrying_the_session_drawer_label_and_its_operator_text`
- [ ] `stamps_every_notification_with_the_moment_it_happened`
- [ ] `streams_an_activity_event_that_telegram_would_never_have_sent`
- [ ] `rejects_a_notification_stream_opened_without_a_valid_session_token`
- [ ] `carries_every_session_on_the_daemon_over_a_single_subscription`
- [ ] `delivers_the_same_notification_to_every_connected_client`

**`packages/tddy-web/cypress/component/SessionNotificationIndicatorsAcceptance.cy.tsx`** —
FR4–FR6 / AC6–AC10, NFR1. Mounts `SessionsDrawerScreen` against a scriptable
`StreamSessionNotifications` feed (`cypress/support/rpc/sessionNotificationFeed.ts`).

- [ ] `blinks a session's dot green while the notification stream reports agent activity`
- [ ] `turns a session's dot yellow when the notification stream reports attention required`
- [ ] `prefers the yellow dot over the blinking one when the agent works and then asks`
- [ ] `settles the dot to steady green once the operator selects the session`
- [ ] `raises the blinking dot again when activity lands after the session was selected`
- [ ] `leaves every other row untouched when one session raises attention`
- [ ] `keeps an inactive session's dot grey however its notifications read`
- [ ] `opens one notification stream for a drawer of many sessions`

### Unit / Integration Tests

**`packages/tddy-daemon/tests/session_notification_bus_unit.rs`** — classification table + fan-out.

- [ ] `classifies_waiting_for_input_as_attention_required`
- [ ] `writes_the_waiting_for_input_notification_in_the_words_telegram_already_uses`
- [ ] `classifies_done_as_attention_required`
- [ ] `writes_the_done_notification_in_the_words_telegram_already_uses`
- [ ] `classifies_a_running_status_as_activity`
- [ ] `classifies_executing_tool_as_activity`
- [ ] `classifies_a_started_session_as_activity`
- [ ] `raises_no_notification_for_a_session_that_ended`
- [ ] `raises_no_notification_for_a_status_it_does_not_recognise`
- [ ] `raises_no_notification_for_an_empty_status`
- [ ] `describes_an_agent_tool_call_as_activity_naming_the_tool`
- [ ] `delivers_a_notification_to_every_subscriber_that_wants_it`
- [ ] `withholds_a_notification_from_a_subscriber_that_does_not_want_it`
- [ ] `delivers_each_notification_in_the_order_it_was_published`
- [ ] `publishing_to_a_bus_with_no_subscribers_is_a_no_op`
- [ ] `keeps_delivering_to_the_remaining_subscribers_when_one_fails`
- [ ] `records_the_session_each_notification_belongs_to`

**`packages/tddy-daemon/tests/telegram_notification_subscriber_unit.rs`** — Telegram's interest
filter and the routing/dedupe that must survive the extraction (FR7).

- [ ] `wants_an_attention_notification_from_the_activity_status_path`
- [ ] `declines_an_activity_notification`
- [ ] `declines_an_attention_notification_raised_by_the_presenter`
- [ ] `declines_everything_when_telegram_is_not_configured`
- [ ] `sends_only_to_the_chats_tracking_the_session_when_any_do`
- [ ] `falls_back_to_the_broadcast_list_when_no_chat_tracks_the_session`
- [ ] `sends_the_notification_text_verbatim`
- [ ] `sends_nothing_when_no_chat_tracks_the_session_and_the_broadcast_list_is_empty`
- [ ] `does_not_send_the_same_attention_notification_twice_in_a_row`
- [ ] `sends_again_once_the_session_has_moved_on_and_come_back`
- [ ] `keeps_two_sessions_dedupe_state_apart`

**`packages/tddy-daemon/tests/session_notification_label_unit.rs`** — the disk lookup feeding the
shared label rule.

- [ ] `names_a_session_after_the_worktree_recorded_in_its_metadata`
- [ ] `falls_back_to_the_short_session_id_when_the_session_records_neither_worktree_nor_goal`
- [ ] `falls_back_to_the_short_session_id_when_the_session_directory_is_missing`
- [ ] `ignores_a_worktree_path_recorded_as_the_empty_string`

**`packages/tddy-web/src/lib/sessionIndicator.test.ts`** (`bun:test`) — the four-state truth
table, including the 30-second decay that Cypress cannot afford to wait for.

- [ ] 19 cases across: liveness first · nothing outstanding · the blink window and its inclusive
      boundary · attention newer than the last view · viewing clears what viewing can clear ·
      `pending_elicitation` is not dismissible by looking at it.

**`packages/tddy-web/src/components/sessions/sessionNotificationRegistry.test.ts`** (`bun:test`).

- [ ] 17 cases across: recording · newest-wins on out-of-order events · per-session isolation ·
      the `useSyncExternalStore` notify contract (including reference stability and no-op writes) ·
      `reset()`.

**`packages/tddy-web/src/utils/sessionDrawerLabel.test.ts`** — two cases added for the placeholder
rule below.

- [ ] `treats the display placeholder as an absent workflowGoal`
- [x] `still prefers the repoPath basename over the display placeholder` — already passes; kept as
      a guard that amending the rule does not disturb the basename branch.

### Test Level Decisions

| Aspect | Level | Rationale |
|---|---|---|
| Label derivation rule | Unit (Rust + TS) | Pure three-step function; the TS side already has the rule, the Rust side must match it exactly. Cheapest place to pin parity. |
| Status → kind classification | Unit (Rust) | Pure mapping over `SessionActivityStatus` wire strings; a table of cases, no I/O. |
| Bus fan-out and `wants` filtering | Unit (Rust) | In-memory recording subscribers; the whole point is that a publish reaches N consumers, which needs no daemon. |
| Hook → Telegram + indicator, end to end | Integration (Rust) | The regression class is the *wiring* at `report_session_status`; only a real `ConnectionServiceImpl` with a real session dir proves it. |
| `StreamSessionNotifications` | Integration (Rust) | The stream's contract is what the web will consume; asserted against the service impl, not the wire. |
| Indicator state derivation | Unit (vitest) | Four states × seen-baseline × 30s window is a truth table. Exact, instant, and impossible to express in Cypress without burning 30 seconds. |
| Notification registry | Unit (vitest) | Stateful store with per-session isolation and a newest-wins rule. |
| Dot rendering, blink class, clear-on-select | Cypress component | The seam is `data-status` + the animation class on a real DOM node, driven by a real stream through `mountWithRpc`. |

### Deliberately not tested

- **The 30-second decay in Cypress.** A component test that waits out the blink window would
  cost 30s against a 10s ceiling. The window is pinned exactly in the vitest truth table; the
  Cypress specs assert only the transitions a stream frame causes.
- **Live Telegram delivery.** As today, `InMemoryTelegramSender` records sends; no network.

## Review findings fixed before merge

Three validation passes ran over the finished change. What they caught, and what was done:

- **A cross-user leak on the new stream.** `stream_session_notifications` authenticated the token
  but never resolved `os_user_for_github`, and the broadcast behind it is host-wide — so on a
  multi-user daemon it fed one operator another's session ids, repository names and prose, and a
  token mapped to no OS user (which `ListSessions` rejects) still received everything. The
  notification now carries its owning `os_user` (server-side only, dropped in the proto conversion),
  the handler authorizes as `stream_session_activity` does, and the relay delivers only on a
  positive owner match — an unowned notification reaches nobody.
- **A targeted alert could become a broadcast.** `chats_tracking_session` returned `Vec::new()` on a
  poisoned tracking mutex, which the subscriber read as "nobody claimed this session" and answered
  by broadcasting to every configured chat. It returns `Option` now; `None` means unknown and sends
  nothing, as the pre-extraction code did.
- **A workflow session's dot never moved without Telegram.** The presenter observer was spawned only
  when `self.telegram` was set, so on a daemon with no `telegram:` block no presenter notification
  was ever published and FR5's presenter half was unreachable. The observer now takes its two sinks
  independently and declines only when neither exists.
- **A per-tool-call file-parse tax.** `resolve_session_label` runs on every reported hook — twice per
  agent tool call — and parsed `.session.yaml` three times (plus `changeset.yaml`) behind an
  unconditional `log::info!`. It now asks the worktree first (`label_from_repo_path`) and pays for
  the enrichment only when the goal will actually be consulted; the log line is `debug!`.

Test-integrity fixes: the AC10 Cypress case asserted its own setup with no delivery barrier (it
passed with a nonsense session id); the label-resolver suite exercised only `claude-cli` sessions,
whose `workflow_goal` the enrichment hard-codes empty, so it would have passed with the lookup
deleted; `notification_for_presenter_event` had no tests at all; and the deleted
`alert_routes_only_to_tracking_chat` had taken "a chat tracking a *different* session is not a
recipient" with it. All four are now covered. The drawer dot was also collapsed into one
`SessionIndicatorDot` — the colour map was shared but the JSX was not, and the collapsed strip's dot
carried no test id, leaving the half that had drifted before still untested.

## Documentation

Updated in this changeset (product docs, `docs/ft/`):

- `docs/ft/daemon/telegram-notifications.md` — **Purpose**, **Message content**, **Message copy**
  and **Implementation surface** all stated that a Telegram message names a session by its uuid
  prefix, and documented the now-deleted `on_claude_cli_activity_status_changed`. Rewritten to say
  which surfaces use the drawer label and which still use the short id, and to describe the bus and
  its subscriber.
- `docs/ft/web/session-drawer.md` — **Connection Status Token** described three dot states from
  `connectionStatusForSession`. Now **Indicator Token**: four states from `sessionIndicatorFor`,
  with a note on what `connectionStatusForSession` still legitimately answers.

**Pending — `packages/*/docs/` is not editable from a changeset** (CLAUDE.md); these are for
`/wrap-context-docs` to carry over:

- `packages/tddy-web/docs/inactive-session-activities.md:21` — "The predicate change also reaches
  the drawer status dot (`SessionDrawerItem`, `SessionDrawer`)". The dot's rule is now
  `sessionIndicatorFor`; `connectionStatusForSession` still drives the partition and base view, so
  only the dot clause is stale.
- `packages/tddy-daemon/docs/telegram-notifier.md` — records the public hooks of
  `telegram_notifier`, including the removed method.

## Technical Debt

- `session_telegram_label` survives for inbound callback-data keys; two id-shortening rules now
  coexist in `telegram_session_control.rs` (`claude-{id[..8]}`, `cursor-cli/{short_id}`).
- The presenter's elicitation keyboard path still sends its own Telegram messages rather than
  going through the bus (see the PRD for why). Tracked under Future Enhancements.
- `useSessionActivity.ts` remains unused; this changeset adds a second activity feed rather than
  adopting it, because it is per-session and this one is daemon-level.
