# 2026-07-25 — pr-stack-live-status: `StackNode.branch` is now assigned at creation from `branch_suggestion` (`planned_prs_into_stack_nodes` + `add_planned_pr_node`), making the branch the durable link key. `orchestrate_pr_stack::github` adds `PrState`, `PrView`, `pr_state_from_github(state, merged_at, draft)`, and `GithubPrApi::get_pr_by_head` (REST, `state=all`, token-gated). New `pr_stack::repoint_planned_pr_node(session_dir, repo_root, node_id, default_branch, gh)`

**Type:** Feature

drops merged parents, rebases the local branch onto the effective base (skipped when not local), and re-targets the open PR base; `git_ops::local_branch_exists` added. Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
