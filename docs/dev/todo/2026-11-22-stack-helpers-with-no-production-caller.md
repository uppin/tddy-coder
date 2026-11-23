# 2026-11-22 — Stack helpers with no production caller after the pr-stack recipe's removal

**Category:** Flagged debt (`#workqueue` 1/14, PR #1002)
**Source:** `/validate-changes` on #1002, re-measured against every caller in `packages/`

When the `pr-stack` workflow recipe was removed, the stack helpers it drove moved into
`tddy-pr-stack` so the daemon's stack tools would keep working. A caller scan at wrap found that the
daemon's tools do **not** reach several of them — `pr_merge` and `pr_repoint` run their own git and
journal steps (`tddy-daemon-rpc` `exec_tool/stack_git_tools/git_work`, `stack_ops::repoint_planned_pr_node`):

| Item | Production callers | Test callers |
|---|---|---|
| `tddy_pr_stack::merge_repoint::{execute_stack_merge, execute_stack_repoint}` | none | `tddy-workflow-recipes/tests/{orchestrate_pr_stack_merge_repoint_acceptance,pr_stack_repoint_acceptance}.rs` |
| `tddy_pr_stack::op_journal::recover_in_flight_stack_op` | none | `tddy-workflow-recipes/tests/orchestrate_pr_stack_{merge_repoint,resume}_acceptance.rs` |
| `tddy_pr_stack::assess::{decide_next_action, OrchestratorAction}` | none | `tddy-workflow-recipes/tests/orchestrate_pr_stack_loop_acceptance.rs` |
| `tddy_pr_stack::docs::{StackDocsOutput, NodeDocs, validate_stack_docs, write_stack_docs}` | none | `docs.rs` unit tests |

(`StackOpJournal` and `MergePhase` are used by the daemon's journal and stay live; so does
`internal_status`.)

Per-PR documents are in the same position from the other side: their layout, listing and the
daemon's attached-changeset prompt line are shipped, but nothing writes `artifacts/prs/<node_id>/`
and no spawn path attaches them (`tddy_session_files::stack_doc_attachments::stack_doc_attachments`
is reached only from tests).

**Why not deleted in #1002:** `#workqueue` n14 (land-propagate) plans to rewrite
`execute_stack_repoint` into the propagation engine, and a `pr_write_node_docs` tool would reuse the
document writer and validator; deleting them under those plans would make the later nodes recreate
them.

**What would close it:** n14 either adopts `merge_repoint` / `recover_in_flight_stack_op` /
`decide_next_action` as the engine's core or deletes them with their tests; per-PR documents get a
writer (`pr_write_node_docs`) and an attaching spawn path (`pr_spawn_child` or the stack view's
Start-session dialog), or the feature is retired. The tests that exercise these through
`tddy-workflow-recipes/tests/` belong in `tddy-pr-stack` either way.
