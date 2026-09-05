# 2026-08-29 — the wire contract for session worktree sync

**Type:** Feature

`AgentActivityRecord` gains `head_commit`, `activity_seq` and `changed_paths`, so a consumer can know what state an edit applied to and which tick carries its patch. Two server-streaming RPCs: `StreamAgentActivityDelta` with `DeltaScope` (a call's own files, a whole tick, or the residual no call declared) returning `AgentActivityDeltaChunk`, and `StreamReadWorktreeFile` returning `WorktreeFileChunk` — bytes rather than the `string content_utf8` that hard-failed on any non-UTF-8 byte and truncated at 1 MiB. New `session_activity.rs` holds the `session.activity` topic constant beside `worktree_activity.rs`, keeping topic and payload schema in the one crate every participant depends on. Feature [session-worktree-sync.md](../../../../docs/ft/daemon/session-worktree-sync.md).
