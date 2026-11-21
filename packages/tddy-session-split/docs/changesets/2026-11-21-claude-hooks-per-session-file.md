# 2026-11-21 — Split agent hooks stay in the private context dir

**Type:** Fix

`agent_argv::split_hooks_file_in(context_dir)`: the split launch writes the hooks the adapter renders
to `context/.claude/settings.local.json`, the file the `host_agent` variant's `hooks_flag` loads,
whatever path the adapter gives them in a worktree. The context dir is the session's own, so no other
session can take that file.

Cross-package entry: [2026-11-21-claude-hooks-per-session-file](../../../../docs/dev/changesets/2026-11-21-claude-hooks-per-session-file.md).
