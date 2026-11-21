# 2026-11-21 — Claude hooks file per session; release of the legacy shared file

**Type:** Fix

`claude_hooks::session_file`:

- `claude_session_hooks_file(workdir, session_id)` gives `<workdir>/.tddy/hooks/<session_id>/claude-settings.json`.
- `release_legacy_claude_settings(bytes) -> LegacyRelease::{Keep, Rewrite, Remove}` takes the
  daemon's `session-hook` and hook-log commands and the route `env` out of a legacy
  `.claude/settings.local.json`, keeping every other key.

Cross-package entry: [2026-11-21-claude-hooks-per-session-file](../../../../docs/dev/changesets/2026-11-21-claude-hooks-per-session-file.md).
