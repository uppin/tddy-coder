# 2026-09-09 — The first delta is tick 1, and this binary stops linking a Telegram bot framework

`StreamAgentActivityDelta` moved to `activity.ActivityService`, and `decide_record` no longer reads a
literal `0`: it reads `tddy_service::session_activity::NO_TICK`, and the first delta a session
produces is now `FIRST_TICK` (1).

That closes a real defect rather than renaming one. `PollState::next_delta_seq` started at 0, which
`decide_record` read as `IgnoreReason::NoTickYet` — "no poll tick has measured this call yet" — so
the **first** change any session made produced a delta no mirror could act on, and its content only
reached the mirror at the next reconcile. Both LiveKit suites worked around it by warming the room
before a client attached. The fix was taken here because `StreamAgentActivityDelta` was getting a new
proto anyway, and a known ambiguity carried into a fresh schema becomes permanent.

**Where the three symbols live is this binary's dependency tree.** They first landed in
`tddy-session-activity`. This crate is installed standalone and depended on `tddy-livekit` and
`tddy-service` only; reaching into that crate for a `const u64 = 0` pulled in `teloxide`,
`teloxide-core`, `teloxide-macros`, `tddy-telegram` and `tddy-github`. They now sit in
`tddy_service::session_activity`, and `cargo tree -p tddy-session-sync | grep -ci teloxide` goes
**3 → 0**.

`STREAM_DELTA_METHOD` is `pub` and read by its twin rather than re-spelled as a literal — one
spelling per coordinate, the same rule the service name follows.

Full record: [../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-session-agent-services.md).
