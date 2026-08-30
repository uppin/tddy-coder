# PRD: Session notifications as indicators

**Created:** 2026-08-29
**Product Area:** daemon (with a `tddy-web` surface)
**Status:** WIP

## Summary

Extract the daemon's session-notification path out of the Telegram notifier into a
**notification bus with pluggable subscribers**, make Telegram name a session exactly the way
the `tddy-web` session drawer names it, and add a second subscriber — `tddy-web` — that turns
those same notifications into per-row **indicator states**: blinking green while the agent is
working, yellow when the agent needs attention.

## Background

Telegram is today the only useful session-notification surface, and it is not a subsystem — it
*is* the notification system. Three problems follow from that.

**1. Telegram names sessions differently from the UI.**
`session_telegram_label` (`packages/tddy-daemon/src/telegram_notifier.rs:51`) labels a session
with the first two hyphen-separated segments of its UUID — `018f1234-5678`. The web's
`sessionDrawerLabel` (`packages/tddy-web/src/utils/sessionDrawerLabel.ts:11`) labels the same
session with the basename of its `repoPath`, falling back to `workflowGoal` and then to the
first eight characters of the session id. An operator reading `🔔 Session 018f1234-5678` in a
chat cannot tell which row in the drawer it belongs to. There is **no shared label rule in
Rust at all** — the TUI, the PR-stack panel, and the Telegram session list each invented their
own.

**2. There is no notification abstraction to subscribe to.**
The daemon has topic-specific broadcast hubs (`AgentActivityHub`, `SessionAgentRoster`,
`WorktreeSizeUpdate`), but nothing that represents *"something happened in a session that an
operator should know about"*. The one place that decides this —
`TelegramSessionWatcher::on_claude_cli_activity_status_changed`
(`packages/tddy-daemon/src/telegram_notifier.rs:421`) — classifies the event, renders the copy,
routes the recipients and performs the send in one method. A second consumer cannot be added
without reaching into the Telegram notifier.

**3. The web cannot see activity it does not already have focused.**
The drawer's dot is derived purely from `ListSessions` fields on a 2-second poll
(`connectionStatusForSession`: green when `is_active`, yellow when `pending_elicitation`, grey
otherwise). The only live activity feed the web opens — `StreamAcpReplay` in
`COUNT_THEN_LIVE` — runs **only for the focused session**
(`packages/tddy-web/src/components/chat/useAcpReplay.ts:99`). An operator watching a drawer of
twelve sessions has no way to tell which one is working and which one is waiting on them.

## Requirements

### Functional Requirements

- [ ] **FR1 — One session label, everywhere.** A single Rust function derives a session's
      human-readable label using the same rule `sessionDrawerLabel` uses: basename of
      `repo_path` → `workflow_goal` → first eight characters of `session_id`. Telegram
      notifications use it, so a chat message and a drawer row name the same session
      identically.
- [ ] **FR2 — A notification bus with subscribers.** Session notifications are published to a
      bus as domain events (`session_id`, `label`, `kind`, `text`, `source`, `at_unix_ms`).
      Subscribers declare which kinds they want and receive only those. Telegram becomes one
      subscriber among several; adding another requires no change to the Telegram notifier.
- [ ] **FR3 — A notification stream for the web.** A daemon-level
      `ConnectionService.StreamSessionNotifications` RPC streams every session's notifications
      to a connected client — one stream for all of the daemon's sessions, not one per session.
- [ ] **FR4 — Blinking green means the agent is working.** A session whose most recent
      notification is an `ACTIVITY` event within the blink window renders a green dot that
      fades in and out.
- [ ] **FR5 — Yellow means attention is required.** A session that raised an
      `ATTENTION_REQUIRED` notification renders a yellow dot. Every notification that reaches
      Telegram also raises yellow: `WaitingForInput`, `Done`, and presenter elicitations.
- [ ] **FR6 — Viewing a session settles its dot.** Selecting a session in the drawer marks its
      notifications seen; the dot returns to steady green. Activity arriving afterwards raises
      blinking green again, and a later attention notification raises yellow again.
- [ ] **FR7 — Telegram traffic is unchanged.** No new Telegram message is sent as a result of
      this work. `ACTIVITY` notifications are consumed by indicator subscribers only; the
      Telegram subscriber takes `ATTENTION_REQUIRED` events from the activity-status path and
      renders today's copy verbatim.

### Non-Functional Requirements

- [ ] **NFR1 — No per-session fan-out cost in the browser.** One `StreamSessionNotifications`
      subscription serves the whole drawer, whatever the session count.
- [ ] **NFR2 — Blink respects `prefers-reduced-motion`.** Following the convention already set
      by `connectionTerminalChromeDotStyles.ts`, the animation is disabled under reduced
      motion, and the dot stays fully opaque.
- [ ] **NFR3 — Publishing never fails a hook.** A notification publish that cannot be delivered
      is logged, not propagated: `ReportSessionStatus` and `ReportAgentActivity` keep returning
      `ok` when a subscriber errors, exactly as the Telegram send does today.
- [ ] **NFR4 — Secrets stay out of notifications.** A notification's `text` is operator-facing
      copy; bot tokens and session tokens never appear in it or in the bus's log lines.

## Indicator model

One dot per drawer row, four states, evaluated in this order:

| State | Rendered | Condition |
|---|---|---|
| `disconnected` | grey, steady | `!is_active` — whatever the session last reported |
| `needs-input` | yellow, steady | `pending_elicitation`, **or** an `ATTENTION_REQUIRED` notification newer than the last view |
| `working` | green, **fading in and out** | an `ACTIVITY` notification newer than the last view, landed within the blink window |
| `connected` | green, steady | active, nothing outstanding |

**Blink window: 30 seconds.** Activity older than that settles the dot to steady green without
any further signal, so a session whose agent died mid-turn stops claiming to be working.

**Decision — `pending_elicitation` is not dismissible by viewing.** FR6 clears the
notification-driven attention state, but a session with `pending_elicitation` set still has an
unanswered gate; clearing its dot on a glance would make the drawer claim the operator had
dealt with something they had not. That flag therefore keeps its current unconditional yellow
(preserving today's behaviour and its tests) and clears when the elicitation is actually
answered. Notification-driven yellow — the Claude CLI `WaitingForInput` / `Done` alerts this
PRD adds — is the dismissible half.

## Notification kinds and sources

```
SessionNotification { session_id, label, kind, text, source, at_unix_ms }

kind   = ACTIVITY | ATTENTION_REQUIRED
source = ACTIVITY_STATUS   (ReportSessionStatus — claude-cli / cursor-cli hooks)
       | AGENT_TOOL_CALL   (ReportAgentActivity — the agent's own tool loop)
       | PRESENTER         (PresenterObserver.ObserveEvents — tddy-coder workflow sessions)
```

Classification of a reported activity status
(`packages/tddy-core/src/session_activity.rs:17`):

| Status | Kind | Telegram | Indicator |
|---|---|---|---|
| `WaitingForInput` | `ATTENTION_REQUIRED` | ✓ (today's copy) | yellow |
| `Done` | `ATTENTION_REQUIRED` | ✓ (today's copy) | yellow |
| `Started`, `Running`, `ExecutingTool` | `ACTIVITY` | — | blinking green |
| `Ended` | *no notification* | — | — |

**Why the presenter's elicitation keyboards are not a bus subscriber.** Presenter
`ModeChanged` elicitations publish `ATTENTION_REQUIRED` onto the bus so indicators see them,
but the Telegram subscriber deliberately does not consume `source == PRESENTER`. That path
carries inline keyboards, a per-chat elicitation FIFO, and the tracked-session gate — it is
Telegram **session control**, not a plain notification, and it keeps sending through
`telegram_notifier` as it does today. Folding it onto the bus is tracked as a follow-up.

## Acceptance Criteria

- [ ] **AC1** — A Claude CLI session in `/home/dev/my-feature` that reports `WaitingForInput`
      produces a Telegram message naming it `my-feature`, not `018f1234-5678`.
- [ ] **AC2** — A session with no `repo_path` falls back to its `workflow_goal`, and one with
      neither falls back to the first eight characters of its id — the same three-step rule the
      drawer uses.
- [ ] **AC3** — One `ReportSessionStatus` call reporting `WaitingForInput` reaches **both** the
      Telegram subscriber and an indicator subscriber, from a single publish.
- [ ] **AC4** — A session reporting `ExecutingTool` reaches the indicator subscriber and sends
      **no** Telegram message.
- [ ] **AC5** — A client connected to `StreamSessionNotifications` receives an attention event
      carrying the session's drawer label and the operator-facing text.
- [ ] **AC6** — A drawer row whose session streams an `ACTIVITY` notification renders a green
      dot with the fade-in/fade-out animation applied.
- [ ] **AC7** — A drawer row whose session streams an `ATTENTION_REQUIRED` notification renders
      a yellow dot.
- [ ] **AC8** — Selecting that row settles its dot back to steady green, and a subsequent
      `ACTIVITY` notification raises the blinking green dot again.
- [ ] **AC9** — A notification for one session leaves every other row's dot untouched.
- [ ] **AC10** — An inactive session's dot stays grey however its notifications read.
