# 2026-08-14 — a split agent's MCP child logs its session room's worktree activity

**Type:** Feature

after connecting to the room named by `TDDY_REMOTE_LIVEKIT_ROOM`, `tddy-tools` subscribes to the `worktree.activity` topic and emits one `DEBUG` line per event through `tddy_service::worktree_activity::format_worktree_activity_for_log`; an undecodable payload warns rather than being dropped silently. Nothing derives state from an event yet — that is deliberate for this changeset. The decode-and-format step is a separate `worktree_activity_line` so both the valid event and the undecodable payload are pinned in the crate that owns them. Feature [session-room.md](../../../../docs/ft/daemon/session-room.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools, tddy-service)
