# 2026-10-24 — Split start streams, no deadline

**Type:** Feature

`#start-progress` 4/8 ([#737](https://github.com/uppin/tddy-coder/pull/737)). Whole-life keep-alive (`with_activity_keepalive`, `STARTING_ACTIVITY`); the unary `forward_start_session` rides the relaying forward into a discarding sink; `start_forward_deadline` and `tests/start_forward_deadline.rs` deleted. See [module layout](../module-layout.md). Cross-package entry: [docs/dev/changesets/2026-10-24-start-progress-split-stream.md](../../../../docs/dev/changesets/2026-10-24-start-progress-split-stream.md).
