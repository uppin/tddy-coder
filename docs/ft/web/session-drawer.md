# Session Drawer Screen

**Route:** `#/sessions`  
**Component:** `SessionsDrawerScreen` (`packages/tddy-web/src/components/sessions/`)

## Overview

A focused session management screen with a left-side drawer listing all sessions in
chronological order and a detail pane on the right showing either the session's live terminal
(when connected) or its resume controls and metadata (when disconnected).

The screen is a parallel view to the existing `ConnectionScreen` (`#/`). Both routes remain
available; no existing behaviour is changed.

## Layout

```
┌────────────────────┬──────────────────────────────┐
│  SessionDrawer     │  SessionDetailPane            │
│  ─────────────     │                               │
│  ● my-feature      │  [when connected]             │
│  ○ old-branch      │  Terminal container           │
│  ◐ waiting         │                               │
│                    │  [when disconnected]           │
│                    │  Goal / Status / Elapsed …    │
│                    │  [Resume]  [Delete disabled]  │
└────────────────────┴──────────────────────────────┘
```

## Drawer Items

Each `SessionDrawerItem` shows:
- A **status dot**: green (`connected`), grey (`disconnected`), yellow (`needs-input`)
- A **derived label** via `sessionDrawerLabel()`:
  1. `basename(repoPath)` — if non-empty
  2. `workflowGoal` — fallback
  3. `sessionId.slice(0, 8)` — last resort
- A **tooltip** on focus/hover revealing the full session id

Sessions are ordered newest-first by `createdAt` (`sortSessionsByCreation()`), with no
active-first grouping (contrast with `sortSessionsForDisplay` used by `ConnectionScreen`).

## Connection Status Token

`connectionStatusForSession(entry)` maps proto fields to one of:
- `"connected"` — `isActive: true` and `pendingElicitation: false`
- `"needs-input"` — `pendingElicitation: true` (takes precedence over `isActive`)
- `"disconnected"` — `isActive: false` and `pendingElicitation: false`

## Session Attachment

`useSessionAttachment` hook manages the single-session attach lifecycle:
- `connectSession` → calls `ConnectSession` RPC → `connected-livekit` or `connected-grpc`
- `resumeSession` → calls `ResumeSession` RPC → same state transitions
- Clicking a connected session in the drawer auto-calls `connectSession`
- Clicking a disconnected session shows the detail pane without auto-connecting

## Routing

```typescript
SESSIONS_DRAWER_ROUTE = "/sessions"
isSessionsDrawerPath(pathname)                     // /sessions and /sessions/:id
sessionsDrawerPathForSession(sessionId)            // builds /sessions/<encoded-id>
parseSessionsDrawerSessionId(pathname)             // extracts decoded id or null
```

## UI Primitives

Adds to `src/components/ui/`:
- `tooltip.tsx` — shadcn Tooltip (Radix `Tooltip` namespace)
- `scroll-area.tsx` — shadcn ScrollArea (Radix `ScrollArea` namespace)

## RPCs Used

No new RPCs. All data comes from existing endpoints:
- `ListSessions` — session list with `isActive`, `createdAt`, `repoPath`, `workflowGoal`, `pendingElicitation`
- `ConnectSession` / `ResumeSession` — attach to a running or paused session
- `StreamTerminalOutput` / `SendTerminalInput` — gRPC terminal stream (claude-cli path)

## Known Limitations

- Delete button is disabled pending a `DeleteSession` RPC wiring in this screen.
- Multi-daemon host filtering (the `daemonInstanceId` grouping in `ConnectionScreen`) is
  deferred — sessions from all daemons appear together in the flat list.
- The old `ConnectionScreen` monolith is not retired by this change.
