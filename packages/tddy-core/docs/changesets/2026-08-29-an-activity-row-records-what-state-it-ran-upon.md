# 2026-08-29 — an activity row records what state it ran upon

**Type:** Feature

`AgentActivityRecord` in `agent_activity.rs` gains `head_commit`, `activity_seq` and `changed_paths`, each `#[serde(default)]` so every `agent-activity.jsonl` written before this still deserializes. Path crediting is here rather than in the daemon because the crate owns the record: a writing tool is credited with the worktree-relative file it named, a tool that declared no write with nothing, and a declared path falling outside the worktree is dropped — a pathspec is what the daemon will diff with, so an absolute or escaping path is not one. Feature [session-worktree-sync.md](../../../../docs/ft/daemon/session-worktree-sync.md).
