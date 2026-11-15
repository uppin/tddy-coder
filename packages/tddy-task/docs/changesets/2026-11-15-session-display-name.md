# 2026-11-15 — An idle tracker can start idle

**Type:** Fix

`IdleTimeoutTracker::last_active_at(timeout, instant)` states how long a tracker has been idle instead of a caller sleeping until it is, so a check that activity reset the clock runs against a minute-long timeout rather than racing a millisecond one.

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
