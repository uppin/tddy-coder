# tddy-pr-stack

The PR-stack data model and its git/GitHub operations: everything that reads or writes a session's
`Changeset.stack`, and the syncs that keep a node's branch and pull request in line with it.

**`tddy-pr-stack` depends on `tddy-core`, `tddy-git`, `tddy-github` and `tddy-workflow`, and must
never depend on `tddy-workflow-recipes`.** The `pr-stack` recipe, its hooks and the plan→stack
bridges stay there and re-export everything below from their historical paths, so a dependency back
would be a cycle. `packages/tddy-workflow-recipes/tests/pr_stack_crate_shape.rs` asserts the seam: no
source file here may name `plan_pr_stack`, `::writer` or `::parser`.

## Module layout

| Module | Owns | Historical path (still re-exported) |
|---|---|---|
| `stack_ops.rs` | add / update / delete / move / repoint / adopt planned nodes, parent rewrites, display order, base-session seeding, `pull_base_into_node_branch`, `sync_node_to_github_pr` | `tddy_workflow_recipes::pr_stack::*` |
| `docs.rs` | per-node `PRD.md` / `changeset.md` paths, writing and validation | `pr_stack::docs` |
| `assess.rs` | `AssessTask`, node views, `decide_next_action`, `effective_base_ref` | `orchestrate_pr_stack::assess` |
| `git_ops.rs` | rebase / merge-base / force-push / commit / push helpers | `orchestrate_pr_stack::git_ops` |
| `pr_insight.rs` | read-side shaping for the PR-inspection tools; `pr_number_from_status_url` | `orchestrate_pr_stack::pr_insight` |
