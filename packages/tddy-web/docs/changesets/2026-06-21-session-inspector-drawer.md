# 2026-06-21 — Session Inspector Drawer

**Type:** Feature

`SessionInspectorDrawer` overlay (open/expanded/closed states, metadata + controls); `inspectorState.ts` pure reducer; `SessionMainPane` (repurposed from `SessionDetailPane`) with inspector toggle; `useSessionAttachment` gains `deleteSession` + `signalSession`; five new proto fields (tool, sessionType, updatedAt, livekitRoom, previousSessionId) rendered; `pendingDelete` reset on session switch via `key` prop. Feature [session-drawer.md](../../../../docs/ft/web/session-drawer.md). (tddy-web)
