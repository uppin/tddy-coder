# tddy-pr-stack

The PR-stack data model and its git/GitHub operations: everything that reads or writes a session's
`Changeset.stack`, and the syncs that keep a node's branch and pull request in line with it. It also
defines the `pr_stack.PrStackService` handler trait, adapter and transport entry.

**`tddy-pr-stack` depends on `tddy-core`, `tddy-git`, `tddy-github`, `tddy-workflow`, `tddy-rpc` and
`tddy-service`, and must never depend on `tddy-workflow-recipes` or `tddy-session-lifecycle`.** The
`pr-stack` workflow recipe is removed; stacks are planned and driven through the project stack record
and the `pr_*` stack tools, which call everything below.

## Module layout

| Module | Owns |
|---|---|
| `stack_ops/` | every writer of `Changeset.stack` and the git/GitHub syncs around it; `mod.rs` re-exports all of it, so callers name only `stack_ops::*` |
| `stack_ops/order.rs` | display order: `assign_missing_display_order`, `move_planned_pr_node` |
| `stack_ops/nodes.rs` | add / update / delete planned nodes, `set_stack_node_parents` |
| `stack_ops/repoint.rs` | `repoint_planned_pr_node` and the rebase / force-push / PR re-target it shares with `set_stack_node_parents` |
| `stack_ops/adopt.rs` | `adopt_pr_as_stack_node`, `adopt_pr_into_stack`, `sync_node_to_github_pr` |
| `stack_ops/pull_base.rs` | `pull_base_into_node_branch`, `BaseSyncStrategy` |
| `docs.rs` | per-node `PRD.md` / `changeset.md` paths, writing and validation |
| `internal_status.rs` | `derive_internal_status`, `reconcile_internal_status` |
| `op_journal.rs` | the crash-safe merge journal: `StackOpJournal`, `MergePhase`, `recover_in_flight_stack_op` |
| `merge_repoint.rs` | `execute_stack_merge`, `execute_stack_repoint` |
| `assess.rs` | `AssessTask`, node views, `decide_next_action`, `effective_base_ref` |
| `git_ops.rs` | rebase / merge-base / force-push / commit / push helpers |
| `pr_insight.rs` | read-side shaping for the PR-inspection tools; `pr_number_from_status_url` |
| `rpc.rs` | `PrStackHandler`, `PrStackServiceImpl`, `build_pr_stack_entry`, `PR_STACK_SERVICE` (also at the crate root) |

Dependencies, the seam tests, each module's surface and what stays recipe-side:
[docs/architecture.md](docs/architecture.md).
