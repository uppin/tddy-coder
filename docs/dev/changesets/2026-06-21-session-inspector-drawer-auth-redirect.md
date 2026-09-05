# 2026-06-21 — Session Inspector Drawer + auth redirect

**Type:** Feature

`tddy-service`: `SessionEntry` proto fields 16–20 (`tool`, `session_type`, `updated_at`, `livekit_room`, `previous_session_id`); `tddy-daemon`: `session_reader`/`connection_service` populate the new fields from `.session.yaml`; `tddy-web`: `SessionInspectorDrawer` overlay (open/expanded/closed, metadata + Resume/Delete/Terminate controls), `inspectorState.ts` reducer, `SessionMainPane`, `useSessionAttachment` `deleteSession`+`signalSession`; all daemon pages now require login with OAuth return-to. PR [#218](https://github.com/uppin/tddy-coder/pull/218); feature [session-drawer.md](../../ft/web/session-drawer.md). (tddy-service, tddy-daemon, tddy-web)
