# 2026-03-14 — Worktree-per-Workflow

**Type:** Feature

Automatic worktree creation after plan approval. fetch_origin_master, create_worktree with start_point, setup_worktree_for_session. build_context_header/prepend_context_header gain repo_dir for agent working directory. WorkflowEvent::WorktreeSwitched; activity log shows worktree path. workflow_runner and daemon use shared core; daemon removes WorktreeElicitation/ConfirmWorktree flow. Planning prompt requires branch_suggestion and worktree_suggestion. (tddy-core)
