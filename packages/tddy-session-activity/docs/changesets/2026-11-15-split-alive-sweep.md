# 2026-11-15 — The `#split-alive` sweep: code issues paid down

**Type:** Refactor

`#split-alive` 5/5, PR [#913](https://github.com/uppin/tddy-coder/pull/913). Behaviour unchanged.

## What changed

- `session_list_status_from_session_dir` delegates to `cli_harness_row`, `state_only_row` and
  `workflow_row` (133 → 57 lines).
