# 2026-11-23 — function-length debt the `#workqueue` stack added is unmeasured

**Category:** Known limitation (measurement gap)

**Source:** `#workqueue` 15/15 sweep (#1022), operator decision D3: `/analyze-code-issues` coverage was not run (cost).

The sweep split the files over 500 lines, but did not measure function length inside files that were under
budget. One known example: `ops::run` in `packages/tddy-daemon-rpc/src/exec_tool/work_queue_tools/ops.rs`
(~186 lines), which the item-edit split did not cut. Run `/analyze-code-issues` over the packages the stack
touched to find the rest.
