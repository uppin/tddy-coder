# 2026-11-15 — The `#split-alive` sweep: code issues paid down

**Type:** Refactor

`#split-alive` 5/5, PR [#913](https://github.com/uppin/tddy-coder/pull/913). Behaviour unchanged.

## What changed

- `spawn_split_agent` takes a `SplitAgentStart` and is cut into `prepare_split_launch`,
  `split_agent_launch_args`, `ensure_split_agent_alive` and `split_start_response` (157 → 51
  lines, 9 → 5 parameters); its code-issue record is deleted.
- `split_claude_extra_args` delegates to `host_variant_args` and `split_denied_tools` (67 → 51).
