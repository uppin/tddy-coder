# 2026-11-22 — Stack helpers with no production caller after the pr-stack recipe's removal

**Category:** Flagged debt (`#workqueue` 1/14, PR #1002; narrowed by `#workqueue` 14/14, PR #1015)
**Source:** `/validate-changes` on #1002, re-measured against every caller in `packages/`

When the `pr-stack` workflow recipe was removed, the stack helpers it drove moved into
`tddy-work-queue` so the daemon's tools would keep working. One row is still open:

| Item | Production callers | Test callers |
|---|---|---|
| `tddy_work_queue::docs::{StackDocsOutput, NodeDocs, validate_stack_docs, write_stack_docs}` | none | `docs.rs` unit tests |

**Closed by #1015 (`#workqueue` 14/14):** `execute_stack_merge` / `execute_stack_repoint`,
`recover_in_flight_stack_op` and `decide_next_action` / `OrchestratorAction` were deleted with
their recipe-era acceptance suites; merging and repointing run only through the land and propagate
engines (`tddy_work_queue::engine`).

**What would close it:** per-PR documents get a writer (a `pr_write_node_docs`-style tool that
calls `write_stack_docs` after `validate_stack_docs`), or the per-PR documents feature is retired
and the writer and validator are deleted with their tests.
