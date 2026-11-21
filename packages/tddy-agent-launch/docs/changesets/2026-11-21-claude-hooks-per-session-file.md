# 2026-11-21 — Resumes render every hooks adapter's hooks; worktree prepared for per-session hooks

**Type:** Fix

- `claude_cli_spawn_steps::resumed_hooks` replaces `resumed_argv_hooks`: a resume rebuilds any
  adapter's hooks around the recorded token, with the resumed route's plain env pinned.
- `session_hooks_worktree::prepare_worktree_for_session_hooks` runs before slot files on start and
  resume: it git-excludes `/.tddy/hooks/` and releases the daemon's entries from the legacy
  `.claude/settings.local.json`, locally or through the target's remote shell.
- `render_host_launch_slots` drops its routed-only `--settings`, which `slots::render` now adds.

Cross-package entry: [2026-11-21-claude-hooks-per-session-file](../../../../docs/dev/changesets/2026-11-21-claude-hooks-per-session-file.md).
