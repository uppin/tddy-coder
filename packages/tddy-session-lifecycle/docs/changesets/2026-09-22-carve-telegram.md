# 2026-09-22 — The Telegram control plane leaves; the presenter observer takes a port

**Type:** Refactor

`#carve` 8/11 ([#494](https://github.com/uppin/tddy-coder/pull/494)). Cross-package entry:
[2026-09-22-carve-telegram.md](../../../../docs/dev/changesets/2026-09-22-carve-telegram.md).

The six Telegram files (7,403 lines, 19% of the crate) moved to
[`tddy-telegram-control`](../../../tddy-telegram-control/README.md), and `teloxide` left the
manifest. Source is 38,960 → 31,664 lines. `DaemonSessionHost`'s
`telegram: Option<Arc<TelegramDaemonHooks>>` became
`presenter_event_sink: Option<SharedPresenterEventSink>`, the `tddy-daemon-kernel` port. The
observer loop stayed here as `presenter_observer_task.rs`, because it also publishes to the
notification bus on daemons with no Telegram. `DaemonSessionHost::new` installs no notification bus;
`runtime.rs` installs the daemon's. See [session-service.md](../session-service.md) § The presenter
observer.

15 suites left with the code: the 12 Telegram suites, `session_notifications_acceptance`,
`session_chaining_phase2_acceptance` and `session_chaining_phase2_unit`. `tests/common/mod.rs`
became `tddy_testing_commons::wait::a_capture_showing`. 83 suites remain; see
[test-suites.md](../test-suites.md).

**Closes** `cycle-connection-service-telegram.md` (1 field, 1 call site, 7,403 lines blocked → 0, 0,
0) and `oversized-file-telegram-session-control.md` (3,980 production lines → the file is gone; its
seven successors in `tddy-telegram-control` peak at 736 and carry their own records).
