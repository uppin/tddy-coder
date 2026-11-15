# 2026-11-15 — The `#split-alive` sweep pays down the stack's code issues

**Type:** Refactor

`#split-alive` 5/5, PR [#913](https://github.com/uppin/tddy-coder/pull/913), the stack's sweep node (#905–#908 are its other nodes). Behaviour is
unchanged: no product statement moves, so there is no feature-doc change.

## What changed

- **B1** — `spawn_split_agent` (`tddy-session-split`) takes a `SplitAgentStart` (who the start is
  for and which two halves it pairs) instead of five loose identity arguments, and is cut into
  `prepare_split_launch` (returning a `SplitLaunch`), `split_agent_launch_args`,
  `ensure_split_agent_alive` and `split_start_response`. Two callers changed
  (`split_claude_cli_start.rs`, `svc_start_sandboxed_codebase_session.rs`); both lost a few lines
  of tail `match` while doing so, so neither grew.
- **B2** — `session_list_status_from_session_dir` (`tddy-session-activity`) delegates its three
  row literals to `cli_harness_row`, `state_only_row` and `workflow_row`, with `read_or_log` and
  `runs_no_workflow` beside them.
- **B3** — `split_claude_extra_args` (`tddy-session-split`) delegates to `host_variant_args` and
  `split_denied_tools`.
- Six of the extracts were applied by `tddy-tools restructure`; the parameter struct, the
  fix-ups and the two row extracts the engine refused (`E0505`, shorthand-field rewrite) were
  written by hand.

## Code issues — final measurement of the deleted record

| Record | Before → after |
|---|---|
| `packages/tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` | 157 lines, 9 parameters → **51 lines, 5 parameters** (nesting 2); deleted |

Functions that were over budget without a record, now within it:
`session_list_status_from_session_dir` 133 → 57, `split_claude_extra_args` 67 → 51. The new
helpers are all within 60 lines, nesting 4 and 5 parameters (largest: `prepare_split_launch`, 60
lines, 5 parameters). `start_split_claude_cli_session` went 170 → 168 and
`start_sandboxed_codebase_session` 90 → 85 (both still over budget; unchanged records).

## Resolved backlog

- `docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md` — item 3 (`spawn_split_agent`)
  removed; item 1 (why the agent exits) stays open.
