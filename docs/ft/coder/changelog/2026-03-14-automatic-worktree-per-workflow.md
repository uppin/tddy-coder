# 2026-03-14 — Automatic Worktree-per-Workflow

- **Worktree creation**: Each TDD workflow automatically creates a git worktree from `origin/master` (after `git fetch`) after plan approval. Branch and worktree names come from the plan agent's `branch_suggestion` and `worktree_suggestion`.
- **Shared core**: Worktree logic lives in tddy-core; both TUI and daemon use `setup_worktree_for_session`. Daemon no longer uses WorktreeElicitation/ConfirmWorktree — worktree is created automatically after ApprovePlan.
- **Context reminder**: Agent prompts include `repo_dir: <absolute path>` when a worktree is active, so agents know their working directory.
- **Activity pane**: Logs worktree path when the switch happens.
- **Packages**: tddy-core (worktree.rs, workflow/mod.rs, tdd_hooks, workflow_runner, presenter), tddy-service (daemon_service).
