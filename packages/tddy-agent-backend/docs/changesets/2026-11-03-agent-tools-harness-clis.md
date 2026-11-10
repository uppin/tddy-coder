# 2026-11-03 — OpenCode, Muse Code and Warp from their specs

**Type:** Feature

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)).

- A launch-side adapter registry, `adapters::launch::LaunchAdapters` (`ModelsAdapter`, `ResumeTokenAdapter`), beside the stream `AdapterRegistry`.
- `opencode-models` (parses `opencode models`) and `warp-resume-token` (the last `warp --resume <TOKEN>` in the exit output).
- `cli_harness.rs`, `cli_harness_muse.rs` and `cli_harness_warp.rs` are no longer on a launch path; their deletion awaits approval ([todo](../../../../docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md)).

See [OpenCode](../../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../../docs/ft/daemon/warp-cli-session.md) CLI sessions.
