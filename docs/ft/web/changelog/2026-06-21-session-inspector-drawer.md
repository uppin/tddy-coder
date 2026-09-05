# 2026-06-21 — Session Inspector Drawer

- `SessionInspectorDrawer` overlay panel: `data-state="closed" | "open" | "expanded"`; header with expand/restore + close buttons; scrollable metadata section (all `SessionEntry` fields, empty omitted); controls (Resume / Delete with two-click confirm / Terminate SIGTERM)
- `inspectorState.ts`: pure `defaultInspectorOpen(isActive)` + `nextInspectorState(state, action)` reducer (actions: open/close/toggle/expand/restore/select)
- `SessionMainPane` (repurposed from `SessionDetailPane`): inspector toggle button, connected-terminal branch, disconnected placeholder; inspector open by default for disconnected sessions
- `useSessionAttachment`: added `deleteSession` (DeleteSession RPC) and `signalSession` (SignalSession RPC) actions
- Proto `SessionEntry` extended with five new fields (tool, sessionType, updatedAt, livekitRoom, previousSessionId) surfaced from `.session.yaml`; `hook_token` never exposed
- Feature: [session-drawer.md](../session-drawer.md)
