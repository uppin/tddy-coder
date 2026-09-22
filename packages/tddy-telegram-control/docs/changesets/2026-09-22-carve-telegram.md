# 2026-09-22 — The crate is created from `tddy-session-lifecycle`'s Telegram control plane

**Type:** Refactor · `#carve` 8/11, PR [#494](https://github.com/uppin/tddy-coder/pull/494)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-telegram.md`](../../../../docs/dev/changesets/2026-09-22-carve-telegram.md)

Created from the six Telegram files of `tddy-session-lifecycle` (7,403 lines, the crate's only
`teloxide` user): `telegram_bot`, `telegram_notifier`, `telegram_multi_select_shortcuts`,
`telegram_session_subscriber`, `telegram_session_control` and `session_notification_subscribers`,
the last renamed `telegram_notification_subscriber`. `telegram_session_subscriber`'s observer loop
stayed behind as `tddy_session_lifecycle::presenter_observer_task`. What it keeps is
`TelegramDaemonHooks` and its `tddy_daemon_kernel::presenter_observer::PresenterEventSink`
implementation. That port is how the connection service reaches Telegram without naming it.

`telegram_session_control.rs` (3,980 production lines) arrived split into seven files, the largest
736. See [architecture.md](../architecture.md). The code moved unchanged, apart from paths and
visibility. Log targets still read `tddy_daemon::…`, and the two untested bot handlers are still
untested. 15 suites came with it, their assertions unchanged. The crate depends on `tddy-telegram`
and `tddy-session-lifecycle`, and neither may depend back.
