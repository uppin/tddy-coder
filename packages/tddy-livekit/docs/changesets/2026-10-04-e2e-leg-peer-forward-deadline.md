# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

Tests only: `tests/rpc_scenarios.rs` is split from one `#[serial]` test running ten scenarios in sequence
into one test per scenario (the ten, plus the existing second test), each starting its own LiveKit
handle. A failure now names its scenario; the assertions are unchanged except one reworded timeout
message in the stateful-bidi test.
