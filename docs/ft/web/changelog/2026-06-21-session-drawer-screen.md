# 2026-06-21 — Session Drawer Screen

- New `#/sessions` route and `SessionsDrawerScreen`: left-side drawer listing all sessions newest-first, detail pane showing terminal (connected) or Resume + metadata (disconnected)
- `SessionDrawerItem`: derived label (`repoPath` basename → `workflowGoal` → `sessionId.slice(0,8)`), status dot (connected / disconnected / needs-input), focus tooltip with full session id
- `useSessionAttachment` hook: single-session `ConnectSession` / `ResumeSession` attach lifecycle, `connected-livekit` and `connected-grpc` states
- New shadcn primitives: `tooltip.tsx`, `scroll-area.tsx`; new utils: `sessionDrawerLabel`, `connectionStatusForSession`, `sortSessionsByCreation`
- Feature: [session-drawer.md](../session-drawer.md)
