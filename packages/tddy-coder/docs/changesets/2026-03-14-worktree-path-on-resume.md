# 2026-03-14 — Worktree Path on Resume

**Type:** Bug Fix

Removed `output_dir` overwrite from `run_plan_resume`, `run_plan_to_complete`, `run_plan_refinement` extra() callbacks. When plan_dir is under `~/.tddy/sessions/`, `plan_dir.parent()` is not the repo root; worktree creation would fail. `build_goal_context` already sets `output_dir` from `changeset.repo_path` when plan_dir is set. (tddy-coder)
