# tddy-pr-stack

The PR-stack data model and its git/GitHub operations: everything that reads or writes a session's
`Changeset.stack`, and the syncs that keep a node's branch and pull request in line with it.

**`tddy-pr-stack` depends on `tddy-core`, `tddy-git`, `tddy-github` and `tddy-workflow`, and must
never depend on `tddy-workflow-recipes`.** The `pr-stack` recipe, its hooks and the plan→stack
bridges stay there and re-export everything below from their historical paths, so a dependency back
would be a cycle. `packages/tddy-workflow-recipes/tests/pr_stack_crate_shape.rs` asserts the seam: no
source file here, at any depth, may name `plan_pr_stack`, `::writer` or `::parser`.

## Module layout

| Module | Owns | Historical path (still re-exported) |
|---|---|---|
| `stack_ops/` | every writer of `Changeset.stack` and the git/GitHub syncs around it; `mod.rs` re-exports all of it, so callers name only `stack_ops::*` | `tddy_workflow_recipes::pr_stack::*` |
| `stack_ops/order.rs` | display order: `assign_missing_display_order`, `move_planned_pr_node` | (via `stack_ops`) |
| `stack_ops/nodes.rs` | add / update / delete planned nodes, `set_stack_node_parents` | (via `stack_ops`) |
| `stack_ops/repoint.rs` | `repoint_planned_pr_node` and the rebase / force-push / PR re-target it shares with `set_stack_node_parents` | (via `stack_ops`) |
| `stack_ops/adopt.rs` | `adopt_pr_as_stack_node`, `adopt_pr_into_stack`, `sync_node_to_github_pr` | (via `stack_ops`) |
| `stack_ops/seed.rs` | base-session seeding: `check_stack_seed_base`, `check_stack_seed_not_self`, `seed_stack_with_base_session` | (via `stack_ops`) |
| `stack_ops/pull_base.rs` | `pull_base_into_node_branch`, `BaseSyncStrategy` | (via `stack_ops`) |
| `docs.rs` | per-node `PRD.md` / `changeset.md` paths, writing and validation | `pr_stack::docs` |
| `assess.rs` | `AssessTask`, node views, `decide_next_action`, `effective_base_ref` | `orchestrate_pr_stack::assess` |
| `git_ops.rs` | rebase / merge-base / force-push / commit / push helpers | `orchestrate_pr_stack::git_ops` |
| `pr_insight.rs` | read-side shaping for the PR-inspection tools; `pr_number_from_status_url` | `orchestrate_pr_stack::pr_insight` |

Dependencies, the seam tests, each module's surface and what stays recipe-side:
[docs/architecture.md](docs/architecture.md).
