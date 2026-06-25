# Session Drawer Screen

**Route:** `#/sessions`  
**Component:** `SessionsDrawerScreen` (`packages/tddy-web/src/components/sessions/`)

## Overview

A focused session management screen with a left-side drawer listing all sessions and a
main content area on the right. A Session Inspector can be opened on the right edge to
show session details and controls; it is hidden by default for connected sessions and
open by default for disconnected sessions.

The screen is a parallel view to the existing `ConnectionScreen` (`#/`). Both routes remain
available; no existing behaviour is changed.

## Layout

```
┌────────────────────┬──────────────────────────────┐
│  SessionDrawer     │  SessionMainPane              │
│  ─────────────     │                               │
│  ● my-feature      │  [when connected]             │
│  ○ old-branch      │  Terminal container           │
│  ◐ waiting         │           ┌──────────────────┤
│                    │           │ SessionInspector  │
│                    │           │ (overlay, ~360px) │
│                    │  [when disconnected]          │
│                    │  "Select a session…"          │
│                    │  (inspector open by default)  │
└────────────────────┴───────────┴──────────────────┘
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
- Clicking a disconnected session opens the inspector by default without auto-connecting

## Session Inspector Drawer

The Session Inspector is an overlay panel anchored to the right edge of `SessionMainPane`.
It shows session metadata and action controls. Its visibility is controlled by a
`data-state` attribute with three values.

### Open/Expand State

- `data-state="closed"` — hidden (default for connected sessions)
- `data-state="open"` — overlay panel ~360px wide, floats over the terminal with a slight
  scrim; does NOT resize/push the terminal
- `data-state="expanded"` — fills the full content area to the right of the session list;
  the session list stays visible so the user can switch sessions

State transitions:
- On session select (connected/active): default `closed`; reset `expanded=false`
- On session select (disconnected/inactive): default `open`; reset `expanded=false`
- Toggle button: `closed → open`, `open → closed`
- Expand button: `open → expanded`
- Restore button: `expanded → open`
- Close button: `open → closed`, `expanded → closed`

### Inspector Header

Title "Inspector", plus expand/restore icon button and close icon button.

### Inspector Metadata Section

All `SessionEntry` proto fields, empty values omitted. Displayed fields:
- Goal (workflowGoal), Status (status), Repo (repoPath), Session ID (sessionId)
- PID (pid, shown when > 0), Workflow state (workflowState)
- Activity status (activityStatus), Agent (agent), Model (model)
- Created (createdAt), Updated (updatedAt)
- Elapsed (elapsedDisplay), Tool (tool)
- Session type (sessionType)
- LiveKit room (livekitRoom)
- Previous session (previousSessionId)

### Inspector Controls Section

Shown below metadata. Actions depend on session state:
- **Resume** — inactive sessions only (ResumeSession RPC)
- **Delete** — all sessions; two-click confirm required (DeleteSession RPC)
- **Terminate** — active sessions only; SIGTERM via SignalSession RPC

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

- `ListSessions` — session list with `isActive`, `createdAt`, `repoPath`, `workflowGoal`, `pendingElicitation`
- `ConnectSession` / `ResumeSession` — attach to a running or paused session
- `StreamTerminalOutput` / `SendTerminalInput` — gRPC terminal stream (claude-cli path)
- `DeleteSession` — delete session (two-click confirm)
- `SignalSession` — SIGTERM for active sessions

## Inspector Tabs

The inspector panel has two tabs:

- **Details** (default) — the existing metadata + controls section described above.
- **Tools** — per-session tool-call log and an inline invoke panel.

### Tools Tab

**Call log**: every `ExecuteTool` RPC call made against the session is durably recorded in
`~/.tddy/sessions/{sessionId}/tool-calls.jsonl` (one JSON record per line, append-only).
The call log survives the in-memory `TaskRegistry` eviction (5 min / 200-entry cap) and
daemon restarts. Each row shows:
- Tool name, status pill (ok / error / running), relative timestamp.
- Expandable detail: **Input** (`args_json`), **Output** (`result_json`), **stdio** (for
  `Shell` tool calls, `stdout` / `stderr` / `exit_code` are embedded in `result_json`).

**Invoke panel**: pick a tool from the `ListExecTools` catalog, edit JSON args seeded from
the tool's `input_schema_json`, click Invoke → calls `ExecuteTool` against the session.
Result or error rendered inline. After a successful invoke the call log auto-refreshes.

**Known limitation**: stdio for background Shell jobs (`block_until_ms: 0`) is non-durable.
Live output is accessible via `TaskService.WatchTask` while the task is in the registry
(~5 min). Once evicted, the log row shows only the `job_id` from `result_json`.

### New RPCs

- `ConnectionService.ListSessionToolCalls(session_token, session_id)` — reads the durable
  JSONL log and returns `ToolCallInfo[]` in chronological order.
- `ConnectionService.ListExecTools` — already existed; used to populate the invoke picker.

## Known Limitations

- The terminal in the main pane is a placeholder; real terminal mounting is out of scope.
- Multi-daemon host filtering (the `daemonInstanceId` grouping in `ConnectionScreen`) is
  deferred — sessions from all daemons appear together in the flat list.
- The old `ConnectionScreen` monolith is not retired by this change.
- Background Shell stdio is not durably captured; only available live via `WatchTask` while
  the task is in the in-memory registry.
