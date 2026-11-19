# 2026-11-18 — `--session-id`, and a live production test of the Coder path

**Type:** Feature · `#coder-start` 9/10, PR [#949](https://github.com/uppin/tddy-coder/pull/949)

Optional `--session-id` lets a caller lay out `<clone>/.worktrees/<id>` for Claude's pre-trust.
New `tests/coder_live_production.rs` (`#[ignore]`) runs the real `coder` CLI; the acceptance suite
uses `tddy_testing_commons::remote_target`. Cross-package record:
[`docs/dev/changesets/2026-11-18-target-test-kit.md`](../../../../docs/dev/changesets/2026-11-18-target-test-kit.md).
