# 2026-10-04 — `restructure_via_index`: the executor that answers restructure tool calls from the warm index

**Type:** Feature

`#live-plan` 14/15, PR [#573](https://github.com/uppin/tddy-coder/pull/573). Cross-package entry:
[2026-10-04-session-restructure-tools.md](../../../../docs/dev/changesets/2026-10-04-session-restructure-tools.md).

`src/restructure_via_index.rs` adds `IndexRestructureExecutor`, a `RestructureExecutor` over an
`IndexChannel`, with one method per tool (`restructure_load`, `_plans`, `_check`, `_apply`, `_status`,
`_anchors`). Every plan and file path goes through `bind_to_session_worktree`. `Check` and `Apply` events
fold into one run object; a mid-stream refusal keeps the operations applied before it, and a
`failed_precondition` is followed by a `PlanStatus` lookup so a stale operation is refused by its id
(`stale_error` carries the message when that lookup fails). See
[index-backed-executor.md](../index-backed-executor.md#the-restructure-tools).

Tests: `tests/restructure_tools_via_index.rs` (14) against a fake `code_index` server on a socket.
