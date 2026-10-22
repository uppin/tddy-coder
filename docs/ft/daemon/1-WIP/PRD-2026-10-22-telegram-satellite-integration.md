# Telegram as a satellite integration - PRD

**Date**: 2026-10-22
**PRD Type**: Technical Improvement (architecture) + Feature (integration status)

## Affected Features

- **Primary Feature**: [Telegram notifications](../telegram-notifications.md) — elicitations and
  attention alerts reach a chat through one **operator channel** port instead of two unrelated seams.
- **Related Feature**: [Session notifications](../session-notifications.md) — the bus delivers to an
  operator channel; the drawer indicator rule is owned by the core, not by Telegram.
- **Related Feature**: [Telegram session control](../telegram-session-control.md) — unchanged
  behaviour; the control plane stays a plugin crate.
- **Related Feature**: [Daemon settings](../daemon-settings.md#integration-status) — Telegram reports
  its live state on the integration status board, and the settings screen shows it.

## Summary

Telegram becomes a **satellite** the way LiveKit did in `#satellite`: the daemon's core defines a
port, the Telegram crates implement it, and only the crate that builds the daemon binary brings the
two together. No core library crate — `tddy-session-activity`, `tddy-session-lifecycle` and the 9
crates that reach Telegram only through them — depends on `tddy-telegram`,
`tddy-telegram-control` or `teloxide`.

The port is about what Telegram *is* to the daemon: **a channel an operator is reached on** — it is
told about a session's elicitations (presenter gates) and about attention-worthy notifications. A
daemon with no Telegram gets the port's named null object.

Telegram also reports its state — off, misconfigured, connecting, connected, unreachable — on the
integration status board, and the daemon settings screen shows it beside LiveKit's.

## Background

Asked 2026-10-22: *which packages depend on telegram?* Eleven workspace crates do, through normal
dependencies (`cargo metadata`). Two core edges carry nearly all of it:

- `tddy-session-activity` → `tddy-telegram`, for two functions that are not Telegram's at all: the
  rule "this presenter mode waits on a human" (it lights the drawer's yellow dot on **every** daemon)
  and a read of `pending_elicitation` from `.session.yaml`. Every crate built on session activity —
  agent launch, CLI sessions, session split, the LiveKit plugin, the demo VM service — compiles
  `teloxide` for them.
- `tddy-session-lifecycle` → `tddy-telegram`, for four `pub use` re-exports whose only reader is the
  daemon's Telegram wiring.

At runtime Telegram already reaches the core through two seams, but they are unrelated and named for
their mechanics: `PresenterEventSink` (kernel) for presenter events and the session-notification
bus's `SessionNotificationSubscriber` (session activity) for alerts. Implementing the second forces
the Telegram plugin to depend on session activity for a trait.

And today a Telegram misconfiguration is silent: `enabled: true` with an empty `bot_token` starts no
bot and tells no one; a revoked token shows only as log lines.

## Proposed Changes

### What's Changing

**1. One port: the operator channel.**
- New `tddy_daemon_kernel::operator_channel`: trait `OperatorChannel` with
  - `on_presenter_event(session_id, &ServerMessage)` — a workflow session's presenter events, in
    stream order (elicitations, state lines); replaces `PresenterEventSink`;
  - `wants_notification(&SessionNotification)` / `on_notification(&SessionNotification)` — the
    attention-worthy notifications off the bus; replaces Telegram's `SessionNotificationSubscriber`;
  - `id()` — the id it reports and logs under.
- Named null object `NoOperatorChannel`: wants no notification, accepts every presenter event.
- `SessionNotification`, `SessionNotificationKind`, `SessionNotificationSource` move to the kernel
  (the port must name them); `tddy-session-activity` re-exports them under their current paths.
- The session host holds a `SharedOperatorChannel`; the daemon installs Telegram's, or
  `NoOperatorChannel`. Session activity offers an adapter that registers a channel on the bus.
- The port lives in the kernel, not in `tddy-daemon-extensions`: it carries `tddy-service`'s
  `ServerMessage`, and the extensions crate must stay free of `tddy-service`.

**2. The core owns the presenter-gate rule.**
- `tddy_daemon_kernel::operator_channel::awaits_operator(&ModeChanged) -> bool` — the one rule for
  "this mode waits on a human" (document review, markdown viewer, feature input, select,
  multi-select, text input). The drawer indicator and Telegram's elicitation FIFO both use it.
- Telegram keeps its message text (`telegram_elicitation_line_for_mode_changed`) and must agree with
  the rule on every mode; `mode_changed_requires_telegram_elicitation` is removed.
- The `pending_elicitation` list flag is read by session activity itself.

**3. The dependency moves to the wiring.**
- `tddy-session-activity` and `tddy-session-lifecycle` drop `tddy-telegram`; lifecycle's four
  re-exports go; `tddy-daemon` names `tddy-telegram` / `tddy-telegram-control` directly for its
  wiring.
- A ratchet test fails if any crate other than the Telegram plugin crates (`tddy-telegram`,
  `tddy-telegram-control`) and the wiring crate (`tddy-daemon`) names a Telegram crate in
  `[dependencies]` or `[build-dependencies]`, and if the kernel or any session crate reaches one
  transitively. `[dev-dependencies]` are not counted, as for LiveKit.

**4. Telegram on the integration status board.**
- Entry `telegram`, label `Telegram`, reported by the Telegram plugin:

  | Config / event | State |
  |---|---|
  | no `telegram:` block, or `enabled: false` | `disabled` |
  | `enabled: true`, `bot_token` empty | `misconfigured` — missing `bot_token` |
  | bot starting, token not yet checked | `connecting` |
  | the bot's identity check succeeded, or a send succeeded | `connected` |
  | the identity check failed, or a send failed | `unreachable` — the last error |

- The error text **never contains the bot token** (Telegram's API URL embeds it).
- The settings screen shows a **Telegram** status line, formatted exactly as LiveKit's
  (`Off`, `Missing: bot_token`, `Connecting…`, `Connected · 3m`, `Unreachable · 12s: …`). A daemon
  that reports no Telegram entry shows no line.

### What's Staying the Same

- Every Telegram behaviour an operator sees in a chat: elicitation keyboards, the per-chat FIFO, the
  tracked-session gate, activity alerts, session start / control, lifecycle messages.
- The drawer indicator on daemons with and without Telegram.
- The `telegram:` config block — no new fields.
- The control plane's structure (`tddy-telegram-control` keeps depending on lifecycle and activity,
  as `tddy-daemon-livekit` does).

## Impact Analysis

**Technical**: 3 manifests lose an edge; `teloxide` leaves the build of 9 library crates.
`DaemonSessionHost::new` and the bus registration change signature (one wiring call site each).
`tddy-daemon-kernel/tests/telegram_extraction_shape.rs` is updated for the renamed port.
**User**: a new status line in settings; misconfiguration and an unreachable bot become visible.
**Risk**: double-send or lost alert if the folded subscriber's `wants` policy drifts — pinned by the
existing Telegram notification suites, which move onto the port.

## Success Criteria

1. `tddy-session-activity` and `tddy-session-lifecycle` name no Telegram crate; the ratchet passes and
   fails on a reintroduced edge.
2. With no `telegram:` block, a workflow session waiting on a select still lights the drawer's
   attention indicator.
3. With Telegram wired, an attention notification reaches the tracked chat once, and a presenter
   elicitation reaches it through the port — same texts as today.
4. `GetIntegrationStatus` lists `telegram` in each state of the table above; the settings screen shows
   the matching line.
5. No reported Telegram error contains the bot token.

## Non-Goals

- Folding presenter elicitations onto the notification bus (keyboards and routing on a notification) —
  `docs/dev/todo/2026-08-29-session-notifications-three-follow-ups-left-open.md`, bullet 1.
- Splitting the oversized Telegram modules (`telegram_notifier.rs`, `telegram_session_control/*`).
- Live reconfiguration of Telegram from the settings screen.
- A second operator channel (Slack, e-mail) — the port allows it; none is built.
