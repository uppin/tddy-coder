# tddy-session-activity

What an agent is doing, what a session's status is, and the transcript of both:
`activity.ActivityService`, eight RPCs. Agent activity records and session-status hooks come in;
live activity streams, session notifications, worktree deltas and ACP transcript replay go out.

Extracted from `tddy-daemon` by `#unbundle` node 7.

## Quick Start

### Testing
```bash
cargo test -p tddy-session-activity
```

⚠ **That command runs nothing.** This crate has no `tests/` directory and no `#[cfg(test)]` module,
across 1,573 production lines. Every assertion about the subsystem is made from
`packages/tddy-daemon/tests/` — the notification, activity-delta and ACP-replay suites stayed there
because each is pinned by `ConnectionServiceImpl` or `test_util::{test_service, TEST_TOKEN}`. That
is a recorded gap, not a design:
[`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../../docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).

## Architecture

Six of the eight methods resolve a session directory the **serving** host derives from the caller's
own token; the two hook reports resolve theirs from the OS user whose per-session `hook_token` they
present. None of them trusts a path or an OS user the request supplied, which is why `ActivityPorts`
carries resolvers rather than values.

Peer routing on `daemon_instance_id` is the daemon's (`PeerRoutedActivity`), as is a session's
display label, which is read from the enrichment that serves `ListSessions` — family C, which stays
in the daemon.

A session's first activity delta is tick **1**, not 0, so it is distinguishable from the wire's
`NO_TICK`. The rule and its compile-time invariant live in `tddy_service::session_activity`, beside
the proto, because the consumer is `tddy-session-sync` — a standalone binary that must not acquire
this crate's dependency tree for three symbols.

`tddy-coder`'s session participant is the **second server** of four of these methods, for
LiveKit-routed sessions, at the same coordinate.

## Documentation

### Product Requirements (What)
- [session-notifications.md](../../docs/ft/daemon/session-notifications.md) — the notification feature
- [agent-activity-pane.md](../../docs/ft/web/agent-activity-pane.md) — the browser's activity and replay panes
- [inactive-session-activities.md](../../docs/ft/web/inactive-session-activities.md) — replay for a session that is not running
- [acp-replay-lazy-tool-bodies.md](../../docs/ft/coder/acp-replay-lazy-tool-bodies.md) — why replay frames omit tool bodies
- [session-worktree-sync.md](../../docs/ft/daemon/session-worktree-sync.md) — what the delta ticks feed
- [telegram-notifications.md](../../docs/ft/daemon/telegram-notifications.md) — the other subscriber

### Technical Implementation (How)
- [activity-service.md](./docs/activity-service.md) — the eight methods, the ports, the tick rule and the routing split
- [agent-activity.md](./docs/agent-activity.md) — the log, the hub, the stream modes, the ACP transcript and the lazy tool bodies
- [session-notifications.md](./docs/session-notifications.md) — the bus, the classification table, the subscribers and their interest filters

## Related Packages
- [tddy-service](../tddy-service/docs/) — owns `activity.proto`, `types.proto` and the tick constants
- [tddy-daemon](../tddy-daemon/docs/connection-service.md) — the host: its ports, its peer routing, its Telegram subscriber and the session label
- [tddy-daemon-livekit](../tddy-daemon-livekit/docs/session-room.md) — the session room whose poll loop fills the delta ring
- [tddy-session-sync](../tddy-session-sync/docs/) — the delta consumer, and the reason the tick rule lives in `tddy-service`
- [tddy-coder](../tddy-coder/docs/) — the second server of families M and N
- [tddy-daemon-kernel](../tddy-daemon-kernel/docs/) — publishes `AgentActivityHub` and `now_unix_ms`
- [tddy-telegram](../tddy-telegram/docs/telegram-notifier.md) — the elicitation classifier the bus reads
