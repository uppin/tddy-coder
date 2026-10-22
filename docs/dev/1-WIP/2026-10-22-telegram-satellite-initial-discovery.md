# 2026-10-22 — Telegram as a satellite — initial discovery

Companion to [`2026-10-22-telegram-satellite.md`](./2026-10-22-telegram-satellite.md). Planning
evidence, not State A; deleted at wrap.

## Combined Conclusions

1. **Who names Telegram today** (`cargo metadata`, 2026-10-22, normal + build edges):
   - direct: `tddy-session-activity` → `tddy-telegram`, `tddy-session-lifecycle` → `tddy-telegram`,
     `tddy-telegram-control` → `tddy-telegram`, `tddy-daemon` → `tddy-telegram-control`;
   - transitive (11): `tddy-agent-launch`, `tddy-cli-sessions`, `tddy-daemon`, `tddy-daemon-livekit`,
     `tddy-daemon-rpc`, `tddy-demo-vm-service`, `tddy-desktop`, `tddy-session-activity`,
     `tddy-session-lifecycle`, `tddy-session-split`, `tddy-telegram-control`. Almost all of it is
     carried by the `tddy-session-activity` edge.
   - dev-only: `tddy-model-registry`, `tddy-worktree-service`, `tddy-tool-engine` (via lifecycle /
     daemon dev-deps). The LiveKit ratchet ignores dev-deps; the developer chose the same here.
2. **The two core edges are not calls into Telegram.**
   - `tddy-session-activity` calls `tddy_telegram::elicitation::mode_changed_requires_telegram_elicitation`
     (`session_notifications.rs:327`, the drawer-dot rule) and
     `tddy_telegram::elicitation::pending_elicitation_for_session_dir` (`session_list_enrichment.rs:295`,
     plus its test at `:930`) — a presenter-gate rule and a `.session.yaml` read. Both must run on a
     daemon with no `telegram:` block.
   - `tddy-session-lifecycle/src/lib.rs` re-exports `active_elicitation`, `elicitation`,
     `telegram_github_link`, `telegram_tracked_session`. Nothing inside lifecycle uses them; the only
     reader is `tddy-daemon/src/runtime/telegram_wiring.rs:67,70` (two coordinator constructors).
3. **The runtime surface is already behind two seams, in two crates.**
   - `tddy_daemon_kernel::presenter_observer::PresenterEventSink` — presenter events (elicitations,
     state lines) of a workflow session. Held as `Option<SharedPresenterEventSink>` by
     `DaemonSessionHost` (`connection_service.rs:94`, `svc_host_builders.rs:36`), handed to the
     observer task in `tddy-session-activity` (`presenter_observer_task.rs:37,90`). Implemented by
     `TelegramDaemonHooks` (`tddy-telegram-control/src/telegram_session_subscriber.rs:33`); also
     constructed inside the control plane (`telegram_session_control/pickers.rs:688`).
   - `tddy_session_activity::session_notifications::SessionNotificationSubscriber` — the bus.
     Implemented by `TelegramNotificationSubscriber` (`tddy-telegram-control/src/telegram_notification_subscriber.rs`)
     with policy `wants = enabled && AttentionRequired && ActivityStatus`, de-dup via
     `LastDeliveredPerSession` (`session_notifications.rs:380`). Registered by the daemon in
     `runtime/mount_host.rs:36-38`.
   Neither seam is named for its job ("an operator is reached through a chat"), and the bus seam
   forces the Telegram plugin to depend on `tddy-session-activity` for the trait and data types.
4. **`tddy-daemon` is already a wiring crate** — listed in `WIRING` in
   `packages/tddy-daemon-extensions/tests/livekit_is_a_satellite.rs`. Everything Telegram in it lives
   in `runtime/telegram_wiring.rs`, `runtime/integrations.rs` (`Telegram::open`), `build_context.rs`,
   `mount_host.rs`, `tasks.rs` (bot dispatcher), `server.rs` (lifecycle message).
5. **Status board.** `tddy_daemon_extensions::IntegrationStatusBoard` / `IntegrationReporter` /
   `IntegrationState {Disabled, Misconfigured{missing}, Connecting, Connected, Unreachable{error}}`.
   Only LiveKit reports (`LIVEKIT_INTEGRATION_ID = "livekit"`). The web reads it in
   `packages/tddy-web/src/components/settings/DaemonSettingsScreen.tsx:58-73` but **picks out the
   `livekit` entry only**; `integrationStatusText.ts` is integration-agnostic. Component spec:
   `packages/tddy-web/cypress/component/DaemonSettingsIntegrationStatus.cy.tsx`.
6. **Today's Telegram states are silent.** `build_telegram` (`telegram_wiring.rs:46-55`) returns
   no hooks for a missing block, `enabled: false`, **and** `enabled: true` with an empty `bot_token`
   — the last is a misconfiguration nobody is told about. `run_telegram_bot` (`telegram_bot.rs:189`)
   starts a teloxide `Dispatcher` and never probes the token; a bad token shows only as log noise.
   `TelegramConfig { enabled, bot_token, chat_ids }` (`tddy-daemon-kernel/src/config.rs:299`).
7. **Port home.** `tddy-daemon-extensions` depends only on `tddy-rpc`/`tokio`/`serde`; carrying
   `tddy_service::gen::ServerMessage` would pull `tddy-service` (and through it `tddy-tui`,
   `tddy-workflow`) into the ports crate. `tddy-daemon-kernel` already depends on `tddy-service` and
   `tddy-daemon-extensions` and already hosts `PresenterEventSink`. The port therefore lives in the
   kernel; the status board stays in extensions.
8. **Existing shape test.** `packages/tddy-daemon-kernel/tests/telegram_extraction_shape.rs` (#carve 8)
   pins `the_connection_service_holds_the_presenter_event_sink_port`,
   `the_only_consumer_of_the_field_goes_through_the_port`, and
   `the_control_crate_depends_on_the_transport_and_the_lifecycle` — the first two change meaning
   when the port is renamed.

## Exploration 1 — dependency graph

- `ls packages | grep -i telegram` → `tddy-telegram`, `tddy-telegram-control`.
- `grep -l -i telegram packages/*/Cargo.toml` → daemon-kernel (comment only, `:45`), daemon,
  session-activity (`:31`), session-lifecycle (`:83`), telegram-control (`:44`), telegram.
- Section check: all four direct edges are `[dependencies]`.
- `./dev cargo metadata --format-version 1` → reverse closure (script in session scratchpad):
  11 crates over normal+build edges (list above); 82 including dev edges (dev-dep cycles through
  test harness crates — noise).

## Exploration 2 — what the core edges call

- `grep -rn tddy_telegram packages/tddy-session-activity/src packages/tddy-session-lifecycle/src packages/tddy-daemon/src`
  → 3 calls in session-activity, 4 `pub use` in lifecycle, the rest in daemon runtime.
- Read `tddy-telegram/src/lib.rs` (module map, `#unbundle` 2 history), `elicitation.rs`:
  `mode_changed_requires_telegram_elicitation(mc) = telegram_elicitation_line_for_mode_changed("_", mc).is_some()`;
  the line function matches `DocumentReview | MarkdownViewer | FeatureInput | Select | MultiSelect |
  TextInput` → Some, `Running | Done` → None, missing mode/variant → None.
  `pending_elicitation_for_session_dir` = `tddy_core::read_session_metadata(dir).pending_elicitation`, false on error.
- Callers of the gate: `session_notifications.rs:327` (`notification_for_presenter_event`) and
  `tddy-telegram-control/src/telegram_notifier.rs:884` (inside `on_server_message`, FIFO rotation).
- `grep` for `crate::active_elicitation|tddy_session_lifecycle::(active_elicitation|...)` → only
  `telegram_wiring.rs:67,70`.
- `notification_for_presenter_event` callers: `presenter_observer_task.rs` (2), lifecycle test
  `tests/session_notification_presenter_unit.rs` (6). `apply_session_list_status_to_proto` callers:
  `tddy-agent-launch/src/session_coordinate_handlers.rs:454` and 7 in-module tests.

## Exploration 3 — the LiveKit pattern

- `packages/tddy-daemon-extensions/README.md`, `src/lib.rs`: ports + named null objects
  (`NoSessionReach`, `NoTerminalExposure`, …) + status board; "a daemon with nothing wired gets each
  port's named null object, which is a decision written down rather than a `None`".
- `tests/livekit_is_a_satellite.rs`: `LIVEKIT_FAMILY`, `PLUGINS`, `WIRING` (`tddy-daemon`,
  `tddy-coder-cli`, `tddy-remote-git-repo-cli`, `tddy-session-sync-cli`, `tddy-tools-cli`, `tddy-rdp`,
  `tddy-vnc`, `tddy-e2e`), uses `tddy_testing_commons::dependency_graph::WorkspaceGraph`
  (`load`, `reaches`, `names_directly`, `workspace_crates`); counts `[dependencies]` +
  `[build-dependencies]` only.
- `docs/dev/changesets/2026-10-17-satellite-1-extension-ports-and-local-connect-session.md` and
  `docs/ft/daemon/1-WIP/PRD-2026-10-16-livekit-satellite-integration.md` for shape.
- `tddy-daemon/src/runtime/integrations.rs`: `Plugins::open` builds the board and the LiveKit
  reporter; `Telegram::open` builds the hooks; `mount_host.rs:185-195` passes
  `telegram_hooks as SharedPresenterEventSink` into `DaemonSessionHost::new`, `:36-38` registers
  `TelegramNotificationSubscriber` on the bus.

## Exploration 4 — status board and web

- `tddy-daemon-extensions/src/integration_status.rs`: `IntegrationState` variants, `reporter(id, label)`,
  `snapshot()` ordered by id.
- `tddy-web/src/components/settings/integrationStatusText.ts` (state → text, throws on unknown),
  `DaemonSettingsScreen.tsx` (polls `getIntegrationStatus` every 5 s, `find(entry.id === "livekit")`,
  `data-testid="daemon-settings-livekit-status"`). No Telegram section on the screen.
- `docs/ft/daemon/daemon-settings.md` § Integration status.

## Exploration 5 — deferred-work scan (Step 2b)

- `grep -rl -iE 'telegram|session-activity|elicitation' docs/dev/todo/` → 10 hits; read:
  `2026-08-29-session-notifications-three-follow-ups-left-open.md` (bullet 1: elicitations bypass the
  bus — related), `2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`,
  `2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`,
  `2026-10-22-telegram-tool-sessions-choose-no-provider-account.md` (unrelated).
- Code issues: `tddy-telegram-control/docs/code-issues/` (11, none claimed — `oversized-file-telegram-notifier.md`,
  `complexity-telegram-notifier-on-server-message.md` cover `:884`), `tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`
  (unclaimed, 525 lines), `tddy-session-activity/docs/code-issues/complexity-service-stream-acp-replay.md`
  (unrelated). `tddy-telegram` and `tddy-daemon` have **no** `docs/code-issues/` directory.
