# 2026-06-21 — **Session Inspector Drawer

**Type:** Feature

surface extra session.yaml fields** — `session_reader.rs` `SessionEntry` struct gains `tool`, `session_type`, `updated_at`, `livekit_room`, `previous_session_id` fields populated from `SessionMetadata`; `connection_service.rs` `list_sessions` maps them to the five new proto fields. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-daemon)
