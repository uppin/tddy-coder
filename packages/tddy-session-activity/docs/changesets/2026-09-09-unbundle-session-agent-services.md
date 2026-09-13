# 2026-09-09 — The crate, and the eight methods it serves

`tddy-session-activity` serves `activity.ActivityService`: agent activity, session status,
notifications and ACP transcript replay. Two modules moved out of `tddy-daemon`
(`session_notifications`, `session_notification_subscribers`), joined by `service` (the eight
handlers) and `streams` (the four live relays).

Six methods resolve a session directory the serving host derives from the caller's token; the two
hook reports resolve theirs from the OS user whose per-session `hook_token` they present. None trusts
a path or an OS user the request supplied, which is why `ActivityPorts` carries resolvers.

**The delta tick numbering changed with this move, deliberately.** A session's first activity delta
was 0, which is also the wire's `NO_TICK`, so a mirror could not tell the first delta from no delta
yet. `StreamAgentActivityDelta` was getting a new proto here, and carrying that ambiguity into a
fresh schema would have made it permanent. The rule lives in `tddy_service::session_activity` —
`FIRST_TICK` is 1, `next_tick` saturates rather than wrapping (a wrap would land back on `NO_TICK`),
and the invariant is a compile-time assertion because a constant nobody passes has no test that would
notice it drifting. It is declared in `tddy-service` rather than here because the consumer is
`tddy-session-sync`, a standalone shipped binary that pulled in `teloxide` and four other crates to
read a `const u64` from this one.

Two seams keep code in the daemon on purpose. A session's **display label** falls back to
`session_list_enrichment`, which serves `ListSessions` — family C, which stays — so
`SessionNotificationPublishing` stays with it and [`SessionLabels`](../../src/service.rs) is the
port. And `TelegramNotificationSubscriber` needs the daemon's config, bot sender and session watcher,
so it implements this crate's subscriber trait from the far side of the boundary, which is what the
trait is for.

⚠ **This crate has no tests** — no `tests/` directory, no `#[cfg(test)]` module, across 1,573
production lines. All sixteen suites covering the subsystem stayed in `tddy-daemon`, each pinned by
`ConnectionServiceImpl` or `test_util`. And nothing anywhere checks that the moved ACP replay output
matches what the old coordinate produced, which for a relocation is the check that matters.

Docs: [activity-service.md](../activity-service.md), [agent-activity.md](../agent-activity.md),
[session-notifications.md](../session-notifications.md).
Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
