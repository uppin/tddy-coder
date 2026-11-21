# 2026-11-21 — `claude-settings-local` renders a per-session file loaded with `--settings`

**Type:** Fix

- `ClaudeSettingsLocal` renders `<workdir>/.tddy/hooks/<session_id>/claude-settings.json`.
- `slots::render` adds the adapter's `file_load_args(hooks_file)` (`--settings <file>`) beside every
  rendered hooks file.
- `HooksAdapter::legacy_shared_file` / `release_legacy_shared_file` name and release the worktree-wide
  file an earlier build wrote.
- Every launch renders hooks of either delivery again.

Cross-package entry: [2026-11-21-claude-hooks-per-session-file](../../../../docs/dev/changesets/2026-11-21-claude-hooks-per-session-file.md).
