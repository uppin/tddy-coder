# oversized-file: exec_tool/stack_git_tools.rs

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/stack_git_tools.rs`
**Category:** oversized-file
**Detected:** 2026-11-14 by `/pr-wrap` step 3.5 file-length gate on #887 (`#orchestrators` 10/11),
production lines counted to the first `#[cfg(test)]` (there is none; the file is all production)
**Metrics:** **999 production lines** · budget 500 · **2.0× over**
**Thresholds breached:** length 999 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — **deferred to the stack's sweep node** (developer instruction 2026-11-14:
no local builds or tests in the wrap run, and the restructure engine needs a build/index;
the split is the sweep node's, after the stack lands) — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-14 | 0 → 999 | created in #887 (`#orchestrators` 10/11): the daemon side of the six stack git tools — the exec-tool handler and `DaemonStackGitTools` (to line ~427), then `GitWork` / `GitEnv`, the node operations (`merge_node`, `close_node`, `repoint_node`, `set_parents`, `resolve_conflicts`) and `merge_tree_conflicts`. A `move_item` deep check was refused by the engine on an unrelated `#[path]` module in this crate (`pr_stack/project_stacks.rs`); an `extract_module` plan for the git-side items (429–999) passed `check --deep` with `pub(crate)` widenings, but its apply was stopped by the developer before any write, so no split landed |

## What would close it

Split along the seam the stopped plan proved: the git-side items (`GitWork`, `GitEnv` and its
`impl`, `run_git_work`, the journal helpers and the five node operations) move to a
`git_work` submodule, leaving the handler, `DaemonStackGitTools` and its `StackGitToolPort`
impl (~430 lines) in `stack_git_tools.rs`; a second move of the node operations out of
`git_work` brings that file under budget too. Prove each seam with
`restructure check --deep`, green baseline before and the same green after
(`./test -p tddy-daemon-rpc`), in the sweep node after the stack lands.
