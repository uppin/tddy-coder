# Changeset: Telegram as a satellite — the operator channel port, the wiring edge, the status board

**Date**: 2026-10-22
**Status**: 🚧 In Progress
**Type**: Architecture Change
**Complexity**: 4 — five crates plus `tddy-web`; a port replacing two seams on the session host; an ordering-sensitive notification policy (no double-send) moved onto the port

## Initial Discovery

[`2026-10-22-telegram-satellite-initial-discovery.md`](./2026-10-22-telegram-satellite-initial-discovery.md)
— the `cargo metadata` closure, every call site of the two core edges, the LiveKit pattern, the
status board and the deferred-work scan.

## Prerequisites

### ⚠ DURING — `telegram_notifier.rs` is 2.5× over budget and `on_server_message` is a complexity hotspot

`packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-notifier.md`,
`packages/tddy-telegram-control/docs/code-issues/complexity-telegram-notifier-on-server-message.md` —
both **unclaimed**. This change edits one line inside `on_server_message` (`:884`, the gate call
becomes `awaits_operator`). Line-neutral; neither record is touched. Not fixed here (non-goal).

### ⚠ DURING — `connection_service.rs` is over budget

`packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` — unclaimed,
525 lines. The `presenter_event_sink` field (`:94`) changes type to `SharedOperatorChannel`; the
field count stays the same. Must not grow the file; re-measure at wrap.

### ℹ ANSWERED (partly) — elicitations bypass the bus — [`2026-08-29-session-notifications-three-follow-ups-left-open.md`](../todo/2026-08-29-session-notifications-three-follow-ups-left-open.md)

Bullet 1. The bus still does not carry elicitations, but after this change both paths end at one
port (`OperatorChannel`), which is the seam a later fold would use. The file stays; at wrap, append
nothing to it — the change history entry names the port as the place the fold would land.

### ℹ — Telegram control plane over budget — [`2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`](../todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md)

Not touched beyond `telegram_notifier.rs:884` and the subscriber fold (`telegram_notification_subscriber.rs`,
under budget). No verdict change.

### Not analyzed

`tddy-telegram` and `tddy-daemon` have **no** `docs/code-issues/` directory.

## Affected Packages

- [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md) — **new** `operator_channel`
  module: `OperatorChannel`, `SharedOperatorChannel`, `NoOperatorChannel`, `awaits_operator`;
  `session_notification` data types moved in; `presenter_observer` removed.
- [`tddy-daemon-extensions`](../../../packages/tddy-daemon-extensions/README.md) — the ratchet
  `tests/telegram_is_a_satellite.rs`; no production change.
- [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md) — drops `tddy-telegram`;
  re-exports the moved notification types; `OperatorChannelSubscriber` bus adapter; the observer
  task delivers to the channel.
- [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md) — drops `tddy-telegram`
  and its four re-exports; `DaemonSessionHost` holds `SharedOperatorChannel`.
- [`tddy-telegram`](../../../packages/tddy-telegram/README.md) — **new** `status` module (integration
  id, configured state, reporting sender, identity check); `elicitation` loses the gate and the
  metadata read.
- [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md) — `TelegramDaemonHooks`
  implements `OperatorChannel` (presenter half + the folded notification subscriber).
- [`tddy-daemon`](../../../packages/tddy-daemon/README.md) — wiring: installs the channel, registers
  it on the bus, reports Telegram on the board, names the Telegram crates directly.
- [`tddy-web`](../../../packages/tddy-web/README.md) — Telegram status line on the settings screen.
- [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md) — none expected (calls
  `apply_session_list_status_to_proto`, whose signature is unchanged); listed because it leaves the
  Telegram closure.

## Related Feature Documentation

- [PRD — Telegram as a satellite integration](../../ft/daemon/1-WIP/PRD-2026-10-22-telegram-satellite-integration.md)
- [Telegram notifications](../../ft/daemon/telegram-notifications.md),
  [Session notifications](../../ft/daemon/session-notifications.md),
  [Daemon settings § Integration status](../../ft/daemon/daemon-settings.md#integration-status)

## Summary

Telegram gets the LiveKit treatment: one core port — the **operator channel** — that a session's
elicitations and attention notifications are delivered through; the Telegram crates implement it;
`tddy-daemon` wires it. `tddy-session-activity` and `tddy-session-lifecycle` stop depending on
`tddy-telegram`, which takes `teloxide` out of 9 library crates, and a ratchet keeps it out. Telegram
also reports its state on the integration status board, shown in settings.

## Background

See the PRD. In short: the two core edges carry a presenter-gate rule and a metadata read that are
not Telegram's, plus four re-exports nobody inside lifecycle reads; and Telegram's two runtime seams
(`PresenterEventSink`, `SessionNotificationSubscriber`) live in different crates under mechanical
names, the second forcing the plugin onto session activity.

## Scope

- [ ] `OperatorChannel` port + `NoOperatorChannel` + `awaits_operator` in `tddy-daemon-kernel`
- [ ] Session-notification data types in the kernel, re-exported from session activity
- [ ] Session activity: channel bus adapter, observer delivers to the channel, own `pending_elicitation` read, no `tddy-telegram`
- [ ] Session lifecycle: host holds the channel, re-exports gone, no `tddy-telegram`
- [ ] Telegram plugin implements the port; the separate notification subscriber is folded into it
- [ ] Telegram reports on the integration status board (config state, identity check, send outcomes), token-free errors
- [ ] Daemon wiring
- [ ] Settings screen Telegram line
- [ ] Ratchet

## Technical Changes

### State A

- `tddy-session-activity/Cargo.toml:31` → `tddy-telegram`; calls at `session_notifications.rs:327`
  (`mode_changed_requires_telegram_elicitation`) and `session_list_enrichment.rs:295`
  (`pending_elicitation_for_session_dir`).
- `tddy-session-lifecycle/Cargo.toml:83` → `tddy-telegram`; `lib.rs` re-exports `active_elicitation`,
  `elicitation`, `telegram_github_link`, `telegram_tracked_session`; read only by
  `tddy-daemon/src/runtime/telegram_wiring.rs:67,70`.
- `tddy_daemon_kernel::presenter_observer::{PresenterEventSink, SharedPresenterEventSink}`, held as
  `Option<…>` by `DaemonSessionHost` and the observer task; implemented by `TelegramDaemonHooks`.
- `tddy_session_activity::session_notifications::{SessionNotification, …Kind, …Source,
  SessionNotificationSubscriber, SessionNotificationBus, LastDeliveredPerSession}`;
  `TelegramNotificationSubscriber` in `tddy-telegram-control` implements the subscriber; the daemon
  registers it (`runtime/mount_host.rs:36-38`).
- Status board: LiveKit only. `build_telegram` silently skips an enabled block with an empty token;
  `run_telegram_bot` never checks the token.
- Web settings: picks the `livekit` entry only.

### State B

```
tddy-daemon (wiring) ──► tddy-telegram-control ──► tddy-telegram ──► tddy-daemon-kernel, tddy-daemon-extensions
       │                         │
       ├──► tddy-session-lifecycle ──► tddy-session-activity ──► tddy-daemon-kernel (OperatorChannel)
       └──► tddy-daemon-extensions (IntegrationStatusBoard)
```

No arrow from a session crate or the kernel into a Telegram crate.

```rust
// tddy_daemon_kernel::operator_channel
#[async_trait]
pub trait OperatorChannel: Send + Sync {
    fn id(&self) -> &'static str;
    async fn on_presenter_event(&self, session_id: &str, event: &ServerMessage) -> anyhow::Result<()>;
    fn wants_notification(&self, notification: &SessionNotification) -> bool;
    async fn on_notification(&self, notification: &SessionNotification) -> anyhow::Result<()>;
}
pub type SharedOperatorChannel = Arc<dyn OperatorChannel>;
pub struct NoOperatorChannel;
pub fn awaits_operator(mode_changed: &ModeChanged) -> bool;

// tddy_daemon_kernel::session_notification — SessionNotification, SessionNotificationKind,
// SessionNotificationSource (moved; re-exported by tddy_session_activity::session_notifications)

// tddy_session_activity::session_notifications
pub struct OperatorChannelSubscriber { /* SharedOperatorChannel */ }
impl OperatorChannelSubscriber { pub fn new(channel: SharedOperatorChannel) -> Self; }
impl SessionNotificationSubscriber for OperatorChannelSubscriber { /* name = channel.id() */ }

// tddy_telegram::status
pub const TELEGRAM_INTEGRATION_ID: &str = "telegram";
pub fn configured_state(telegram: Option<&TelegramConfig>) -> IntegrationState;
pub struct ReportingSender<S> { /* inner sender, reporter, token */ }
impl<S: TelegramSender> ReportingSender<S> { pub fn new(inner: S, reporter: IntegrationReporter, bot_token: &str) -> Self; }
#[async_trait] pub trait BotIdentity: Send + Sync { async fn check_identity(&self) -> anyhow::Result<()>; }
pub async fn report_bot_identity(identity: &dyn BotIdentity, reporter: &IntegrationReporter, bot_token: &str);
pub fn redact_bot_token(text: &str, bot_token: &str) -> String;
```

### Delta

- **kernel**: add `operator_channel.rs`, `session_notification.rs`; remove `presenter_observer.rs`
  (callers move to the port); `tests/telegram_extraction_shape.rs` follows the rename.
- **session-activity**: `session_notifications.rs` re-exports the moved types, gains
  `OperatorChannelSubscriber`, calls `awaits_operator`; `session_list_enrichment.rs` reads
  `pending_elicitation` itself; `presenter_observer_{task,spawn}.rs` take `SharedOperatorChannel`;
  manifest drops `tddy-telegram`.
- **session-lifecycle**: four `pub use` removed; host field + `DaemonSessionHost::new` parameter
  become `SharedOperatorChannel`; manifest drops `tddy-telegram`.
- **telegram**: `status.rs`; `elicitation.rs` keeps the line text and signature, loses
  `mode_changed_requires_telegram_elicitation` and `pending_elicitation_for_session_dir`; manifest
  adds `tddy-daemon-extensions`.
- **telegram-control**: `TelegramDaemonHooks: OperatorChannel` (absorbs
  `TelegramNotificationSubscriber`'s `wants` / `deliver` and its de-dup); `telegram_notifier.rs:884`
  calls `awaits_operator`; `pickers.rs:688` casts to `SharedOperatorChannel`.
- **daemon**: `mount_host.rs` installs `hooks as SharedOperatorChannel` or `NoOperatorChannel`, and
  registers `OperatorChannelSubscriber`; `integrations.rs` creates the `telegram` reporter, reports
  `configured_state`, wraps the sender in `ReportingSender`; `tasks.rs` runs `report_bot_identity`
  before the dispatcher; `telegram_wiring.rs` names `tddy_telegram::…` directly; manifest adds
  `tddy-telegram`.
- **web**: `DaemonSettingsScreen.tsx` shows the `telegram` entry as `daemon-settings-telegram-status`.

## Implementation Milestones

- [ ] Kernel port and moved types compile; kernel acceptance suite green
- [ ] Session activity on the port, no `tddy-telegram`; its suites green
- [ ] Session lifecycle on the port, no `tddy-telegram`; its suites green
- [ ] Telegram plugin implements the port; every `tddy-telegram-control` suite green unchanged in behaviour
- [ ] Telegram status module; its suite green
- [ ] Daemon wiring; `cargo build -p tddy-daemon`
- [ ] Web line; component spec green
- [ ] Ratchet green

## Testing Plan

**Levels.** The dependency rule is a workspace-graph test (the LiveKit ratchet's harness). The port
contract and the gate rule are unit-level in the kernel. Delivery through the bus is an in-memory
integration test in session activity. Telegram's policy on the port reuses the existing
`tddy-telegram-control` suites (which pin texts, routing and de-dup) plus one port-level suite. Status
reporting is unit-level against in-memory senders and identity fakes — a real Telegram API is not
reachable from CI. The web line is a Cypress component test with `mountWithRpc`.

**Options considered.** An end-to-end daemon test for the status entry was rejected: it would need a
Telegram API double over HTTP for one mapping the unit tests already pin; the wiring is three lines,
covered by `cargo build` and review.

**Coverage.** Every `IntegrationState` the PRD's table names; every `AppModeProto` variant for the
gate; both policy exclusions (`Activity`, `Presenter`); token redaction on both failure paths.

## Acceptance Tests

### `tddy-daemon-extensions` — `packages/tddy-daemon-extensions/tests/telegram_is_a_satellite.rs`

- `the_daemon_reaches_the_telegram_plugin` — non-vacuity: the walk sees `tddy-telegram-control` from `tddy-daemon`.
- `only_telegram_plugins_and_wiring_name_a_telegram_crate` — the rule.
- `the_kernel_reaches_no_telegram_crate` — the port crate is clean transitively.
- `no_session_crate_reaches_a_telegram_crate` — `tddy-session-activity`, `tddy-session-lifecycle`,
  `tddy-agent-launch`, `tddy-cli-sessions`, `tddy-session-split`.

### `tddy-daemon-kernel` — `packages/tddy-daemon-kernel/tests/operator_channel_acceptance.rs`

- `every_mode_that_asks_a_question_awaits_the_operator` (rstest: document review, markdown viewer,
  feature input, select, multi-select, text input).
- `an_autonomous_mode_does_not_await_the_operator` (rstest: running, done, no mode).
- `no_operator_channel_wants_no_notification`
- `no_operator_channel_accepts_every_presenter_event`
- `a_presenter_event_handed_to_the_shared_channel_reaches_the_channel_behind_it`

### `tddy-session-activity` — `packages/tddy-session-activity/tests/operator_channel_subscriber_acceptance.rs`

- `a_channel_on_the_bus_receives_the_notification_it_wants`
- `a_channel_on_the_bus_is_not_handed_a_notification_it_declines`
- `the_subscriber_is_named_after_its_channel`
- `a_select_prompt_raises_attention_with_no_channel_wired` — the drawer dot on a Telegram-less daemon.

### `tddy-telegram` — `packages/tddy-telegram/tests/telegram_integration_status_acceptance.rs`

- `a_daemon_without_a_telegram_block_reports_telegram_disabled`
- `a_switched_off_telegram_reports_disabled`
- `an_enabled_telegram_without_a_bot_token_reports_the_missing_token`
- `a_configured_telegram_reports_connecting_until_the_bot_is_checked`
- `a_successful_send_reports_connected`
- `a_failed_send_reports_unreachable_with_the_error`
- `a_failed_send_never_reports_the_bot_token`
- `a_bot_whose_identity_checks_out_reports_connected`
- `a_bot_whose_identity_check_fails_reports_unreachable_without_the_token`

`packages/tddy-telegram/src/elicitation.rs` (`#[cfg(test)]`):
- `the_telegram_line_exists_exactly_for_the_modes_that_await_the_operator` (rstest over every variant).

### `tddy-telegram-control` — `packages/tddy-telegram-control/tests/telegram_operator_channel_acceptance.rs`

- `the_telegram_channel_wants_an_attention_alert_from_the_activity_status_path`
- `the_telegram_channel_declines_activity_and_presenter_notifications`
- `an_attention_alert_reaches_the_tracked_chat_through_the_channel`
- `an_unchanged_alert_reaches_the_chat_once`
- `a_switched_off_telegram_channel_wants_nothing`
- `the_telegram_channel_is_identified_as_telegram`

Existing suites that move onto the port (behaviour unchanged): `telegram_notification_subscriber_unit.rs`,
`session_notifications_acceptance.rs`, `telegram_claude_cli_activity_alert_acceptance.rs`.

### `tddy-daemon-kernel` — `packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs` (updated)

- `the_connection_service_holds_the_operator_channel_port` (was `…_presenter_event_sink_port`).
- `an_event_handed_to_the_shared_port_reaches_the_sink_behind_it` is **moved**, not dropped: it is
  `operator_channel_acceptance.rs`'s `a_presenter_event_handed_to_the_shared_channel_reaches_the_channel_behind_it`.

`packages/tddy-telegram/src/elicitation.rs`'s `mode_changed_requires_telegram_elicitation_select_true_running_false`
is **superseded** by `the_telegram_line_exists_exactly_for_the_modes_that_await_the_operator` (every
variant, against the core rule) — the function it tested is removed.

### Red-run evidence (2026-10-22, scoped)

| Suite | Result | Reason |
|---|---|---|
| `tddy-daemon-extensions --test telegram_is_a_satellite` | 2 pass, 2 fail | names `tddy-session-activity`, `tddy-session-lifecycle`; the 5 session crates reach `tddy-telegram` + `teloxide`. The 2 passing are the non-vacuity and kernel-clean guards |
| `tddy-daemon-kernel --test operator_channel_acceptance` | compile error | `tddy_daemon_kernel::operator_channel`, `::session_notification` unresolved |
| `tddy-daemon-kernel --test telegram_extraction_shape` | 7 pass, 1 fail | `the_connection_service_holds_the_operator_channel_port` |
| `tddy-session-activity --test operator_channel_subscriber_acceptance` | compile error | `operator_channel`, `OperatorChannelSubscriber` unresolved |
| `tddy-telegram --test telegram_integration_status_acceptance` | compile error | `tddy_telegram::status`, `tddy_daemon_extensions` (manifest edge is green's) unresolved |
| `tddy-telegram --lib` | compile error | `tddy_daemon_kernel::operator_channel` unresolved |
| `tddy-telegram-control --test telegram_operator_channel_acceptance` | compile error | `operator_channel`, `session_notification` unresolved |
| `DaemonSettingsIntegrationStatus.cy.tsx` | 8 pass, 2 fail | `daemon-settings-telegram-status` never rendered; the "no Telegram line" case passes today by construction and guards the absent entry |

### `tddy-web` — `packages/tddy-web/cypress/component/DaemonSettingsIntegrationStatus.cy.tsx`

- `shows Telegram's state beside LiveKit's`
- `shows a misconfigured Telegram's missing token`
- `shows no Telegram line when the daemon reports no Telegram entry`

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Port, not a moved function** (developer, 2026-10-22): the port is "an operator is reached through
  a channel" — notifications and elicitations — not a classifier. The gate rule must run without
  Telegram (drawer dot), so the core owns it and Telegram consumes it.
- **Port in the kernel, not in `tddy-daemon-extensions`** — it carries `ServerMessage`; the extensions
  crate stays free of `tddy-service`. The status board stays in extensions. The ratchet lives next to
  LiveKit's in extensions, because that crate already has the graph harness.
- **Null object over `Option`** — matches the extensions convention; the observer still runs when only
  the bus wants its events.
- **Status board included** (developer, 2026-10-22), with token redaction as a hard requirement.
- **Ratchet ignores dev-dependencies** (developer, 2026-10-22), as for LiveKit.
- **One PR** (developer, 2026-10-22).

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-22-telegram-satellite-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-22-telegram-satellite-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
