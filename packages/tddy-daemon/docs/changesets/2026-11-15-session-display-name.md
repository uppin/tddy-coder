# 2026-11-15 — Relay idle tests no longer race the clock

**Type:** Fix

`relay_idle_wired_acceptance` and `relay_e2e_acceptance` build an already-idle tracker with `IdleTimeoutTracker::last_active_at`; the expiry half is its own test (`relay_idle_tracker_expires_after_a_full_idle_period`).

Cross-package entry: [2026-11-15-session-display-name.md](../../../../docs/dev/changesets/2026-11-15-session-display-name.md). PR [#910](https://github.com/uppin/tddy-coder/pull/910).
