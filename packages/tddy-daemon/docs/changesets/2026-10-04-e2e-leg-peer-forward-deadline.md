# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

Tests only: `session_attach_cross_host_acceptance` sets `peer_forward_timeout_secs: 2` through the daemon
config, so the silent-peer deadline test finishes in a few seconds (6.25 s measured) instead of waiting
out 30 s; it still asserts `DeadlineExceeded`, naming 2 s, and its comments about the deadline are
corrected.
