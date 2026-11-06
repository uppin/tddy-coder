# 2026-11-06 — Telegram's hand-built launch is gone

**Type:** Refactor (planned deletions)

`#agent-tools` 18/19 ([#824](https://github.com/uppin/tddy-coder/pull/824)).

- Deleted: `CliSessionManager::{start, start_cursor}` and `build_cursor_argv` (planned), `resume_cursor` (approved beyond the plan, 2026-11-06). `build_claude_argv` and `with_claude_launch_flags` stay for the split agent's `start_with_options`.
- Suites moved from `start` to `start_with_options`; `cli_harness_parity.rs` checks the claude descriptor only.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-telegram-spec-launch.md).
