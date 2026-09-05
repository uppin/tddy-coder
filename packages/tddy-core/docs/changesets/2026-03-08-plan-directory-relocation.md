# 2026-03-08 — Plan Directory Relocation

**Type:** Feature

plan_dir_suggestion in discovery relocates plan dir from staging to git_root/suggestion/dir_name. relocate_plan_dir(), find_git_root(), copy_dir_recursive(). Reject absolute, .., empty. Cross-device copy+delete fallback. System prompt path canonicalization. Planning prompt instructs agent to use plan_dir_suggestion. No symlink. (tddy-core)
