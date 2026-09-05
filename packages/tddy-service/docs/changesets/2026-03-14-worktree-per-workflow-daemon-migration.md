# 2026-03-14 — Worktree-per-Workflow Daemon Migration

**Type:** Feature

Removed WaitingConfirmWorktree state and handle_confirm_worktree. handle_approve_plan calls setup_worktree_for_session directly; worktree created from origin/master after ApprovePlan. No WorktreeElicitation or ConfirmWorktree flow. (tddy-service)
