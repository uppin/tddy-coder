# Session Inspector Drawer

Adds a slide-in inspector overlay to the Sessions Drawer screen that surfaces per-session
metadata (tool, session type, timestamps, LiveKit room, previous session link) and action
controls (resume, delete, terminate). Extends the proto/Rust/TS stack to carry five new
fields from `.session.yaml` through to the UI.

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Red phase tests written
- [ ] Green phase (implementation)
- [ ] PR merged

## tddy-service/proto

- `packages/tddy-service/proto/connection.proto` — extend `message SessionEntry` with five
  new string fields at field numbers 16–20:
  - 16: `tool` (from `.session.yaml` `tool` field)
  - 17: `session_type` (from `.session.yaml` `session_type` field)
  - 18: `updated_at` (from `.session.yaml` `updated_at` field)
  - 19: `livekit_room` (from `.session.yaml` `livekit_room` field)
  - 20: `previous_session_id` (from `.session.yaml` `previous_session_id` field)
  - `hook_token` is NOT included (per-session secret, never expose to the UI)
- Rust codegen: automatic via `packages/tddy-service/build.rs` on `cargo build`.
- TS codegen: `cd packages/tddy-web && bun run generate` regenerates
  `packages/tddy-web/src/gen/connection_pb.ts` with camelCase fields:
  `tool`, `sessionType`, `updatedAt`, `livekitRoom`, `previousSessionId`.

## tddy-daemon

- `packages/tddy-daemon/src/session_reader.rs`:
  - Add five new fields to internal `SessionEntry` struct: `tool: String`,
    `session_type: String`, `updated_at: String`, `livekit_room: String`,
    `previous_session_id: String`.
  - Populate in `list_sessions_in_dir` from `SessionMetadata`:
    `tool.unwrap_or_default()`, `session_type.unwrap_or_default()`,
    `updated_at` (non-Option), `livekit_room.unwrap_or_default()`,
    `previous_session_id.unwrap_or_default()`.
- `packages/tddy-daemon/src/connection_service.rs`:
  - In the `ProtoSessionEntry { … }` literal inside `list_sessions` (~line 732),
    set the five new proto fields from the internal entry.

## tddy-web

**New components** (`packages/tddy-web/src/components/sessions/`):
- `SessionInspectorDrawer.tsx` — overlay panel with header (title + expand/restore +
  close buttons), scrollable metadata section, controls section.
- `inspectorState.ts` — pure module: `defaultInspectorOpen(isActive: boolean): boolean`
  and `nextInspectorState(state, action)` reducer.
  - State: `{ open: boolean; expanded: boolean }`.
  - Actions: `open | close | toggle | expand | restore | select`.
  - `select` action takes `{ isActive: boolean }` and resets to default.

**Modified components**:
- `SessionDetailPane.tsx` repurposed as `SessionMainPane.tsx`:
  - Keep no-selection placeholder and connected-terminal branches.
  - Drop disconnected metadata/controls branch (moves to inspector).
  - Render the inspector toggle button and `SessionInspectorDrawer` overlay.
- `SessionsDrawerScreen.tsx`:
  - Add `inspectorOpen`/`inspectorExpanded` state via `inspectorState.ts`.
  - Default open on select: open for disconnected sessions, closed for connected.
  - Add `handleDelete` wired to `deleteSession` RPC.
  - Add `handleTerminate` wired to `signalSession` RPC (SIGTERM).
  - Pass inspector state + handlers to `SessionMainPane`.

**`useSessionAttachment.ts`**:
- Add `deleteSession(sessionId, sessionToken, client)` action.
- Add `signalSession(sessionId, signal, sessionToken, client)` action.

**Test scaffolding**:
- `cypress/support/testIds.ts` — new static IDs and dynamic helpers for inspector.
- `cypress/support/pages/sessionsDrawerPage.ts` — new page-object methods.
- `cypress/support/rpc/connectionRpcs.ts` — reuse existing `interceptSignalSession`;
  ensure `interceptDeleteSession` exists.

**Acceptance tests** — new file `cypress/component/SessionInspectorAcceptance.cy.tsx`:
1. Connected session selected → inspector hidden by default (`data-state="closed"`).
2. Inspector toggle opens overlay (`data-state="open"`) without hiding terminal.
3. Disconnected session selected → inspector open by default (`data-state="open"`).
4. Expand button → `data-state="expanded"`; restore → `data-state="open"`.
5. Close button → `data-state="closed"`.
6. Metadata renders new fields (tool, sessionType, updatedAt, livekitRoom, previousSessionId).
7. Resume → ResumeSession RPC with correct session id.
8. Delete → confirm step → DeleteSession RPC with correct session id.
9. Terminate → SignalSession with SIGTERM signal for an active session.

Updated: `cypress/component/SessionsDrawerAcceptance.cy.tsx` — AC6 adjusted (metadata/Resume
now in inspector, not the old detail pane).

**Unit tests**:
- `packages/tddy-web/src/components/sessions/inspectorState.test.ts` (bun):
  - `defaultInspectorOpen(false)` returns `true`, `defaultInspectorOpen(true)` returns `false`.
  - Reducer state transitions for all actions.

## tddy-daemon (unit tests)

- `packages/tddy-daemon/src/session_reader.rs` `#[cfg(test)]`:
  - Write a `.session.yaml` with all five new fields → `list_sessions_in_dir` returns them.
  - `hook_token` field is NOT present on `SessionEntry` struct (compile-time check).
- `packages/tddy-daemon/src/connection_service.rs` existing test scaffolding:
  - Extended to verify proto `SessionEntry` carries all five new fields end-to-end.
