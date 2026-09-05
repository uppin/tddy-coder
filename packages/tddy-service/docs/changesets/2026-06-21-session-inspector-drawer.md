# 2026-06-21 — **Session Inspector Drawer

**Type:** Feature

proto SessionEntry extensions** — `connection.proto` `SessionEntry` gains five new string fields at numbers 16–20: `tool`, `session_type`, `updated_at`, `livekit_room`, `previous_session_id` (from `.session.yaml`); `hook_token` excluded; TypeScript codegen regenerated. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-service)
