# 2026-11-25 — coding_agent.activity: where a session's activity is read from, and its transcript's mapping in YAML

**Type:** Feature

`ActivitySlot` (`spec/activity_slot.rs`): `messages` / `tool_calls` each `hooks | transcript | both`, `hook_messages`, and `transcript { path, lines }` read by the generic `TranscriptSource::map_line` (`where`, `parts`, `emit`, `unwrap_tag`); `worktree_slug`; `validate_activity_slot`. `cursor.yaml` reads its messages from its transcript; `claude.yaml` / `muse.yaml` declare their `UserPromptSubmit` / `Stop` messages. Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-source.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-source.md) (#1035).
