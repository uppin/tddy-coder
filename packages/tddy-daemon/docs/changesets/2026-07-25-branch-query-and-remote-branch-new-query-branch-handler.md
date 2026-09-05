# 2026-07-25 — branch-query-and-remote-branch: new `query_branch` handler (reuses the `get_pr_status` prologue + `get_pr_by_head` + branch→session scan + `tddy_core::worktree::worktree_path_for_branch`) resolves `{branch, session, worktree, pr}` for one head branch; `create_remote_branch` is threaded through the three session-start intent helpers so a set flag runs `push_new_branch_to_origin` after `create_worktree_with_retry` and sets `Changeset.remote_pushed = true` (a push failure fails `StartSession`, no fallback). Feature [pr-stack-live-status.md](../../../../docs/ft/coder/pr-stack-live-status.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-daemon)

**Type:** Feature


