# WIP Changeset: Terminal Control Mutex (Single-Screen Input Ownership)

**Feature slug:** `terminal-control-mutex`  
**Branch:** `claude-session-mutex`  
**Status:** Green phase complete — production logic implemented; all tests passing

## Problem / Motivation

A Claude Code CLI session (and any terminal within it) can today receive input from multiple
screens simultaneously — two browser tabs for the same user both send keystrokes to the same
PTY, producing conflicting/interleaved input. There is no mechanism to designate a single
controlling screen.

## Solution

A per-session **control lease** stored in `ClaudeCliSessionManager`. The first screen to claim
the lease receives a `control_token`; subsequent input RPCs validate the token server-side and
reject non-controllers. A `WatchTerminalControl` server-streaming RPC propagates lease changes
so displaced screens can render a **"Claim terminal"** CTA in real time.

## TODO

- [x] Create/update PRD documentation
  - `docs/ft/daemon/terminal-sessions.md` extended with §"Terminal Control Ownership"
  - `docs/ft/web/session-drawer.md` extended with §"Terminal Control — Claim terminal CTA"
- [x] Create changeset (this file)
- [x] Failing acceptance tests written (Step 6)
  - `packages/tddy-web/cypress/component/TerminalControlAcceptance.cy.tsx` (4 CT tests)
- [x] Failing unit/integration tests written (Step 7)
  - `packages/tddy-daemon/tests/terminal_control_acceptance.rs` (10 Rust tests)
  - `packages/tddy-web/src/components/sessions/terminalControlState.test.ts` (5 bun tests)
  - `packages/tddy-web/src/lib/screenId.test.ts` (4 bun tests)
- [x] Implement production logic (`/green`)
  - `ClaudeCliSessionManager.claim_control` / `verify_control` / `current_control` — real lease logic
  - `ConnectionService.claim_terminal_control` / `watch_terminal_control` — real RPC handlers
  - `send_terminal_input` + `stream_session_terminal_io` — control token enforcement gate
  - `applyTerminalControlEvent` reducer — pure state fold
  - `SessionMainPane` — "Claim terminal" overlay CTA
- [x] Code quality review — Score: **B** (all must-refactor items fixed, see below)
  - Extracted `runControlSession` helper from `useTerminalControl` (reduced nesting 5→3, length 81→20 lines in hook)
  - Extracted `relay_control_events` free function from `watch_terminal_control` (nesting 5→3)
  - Extracted `generateScreenId` helper in `screenId.ts` (eliminated duplicated template string)
- [ ] Wrap changeset

## Files Changed (red phase)

### tddy-service
- `packages/tddy-service/proto/connection.proto` — `ClaimTerminalControl` + `WatchTerminalControl`
  RPCs; new messages `ClaimTerminalControlRequest/Response`, `WatchTerminalControlRequest`,
  `TerminalControlEvent`; `control_token` field on `SessionTerminalInput` (5),
  `SignalSessionRequest` (4), `StartTerminalSessionRequest` (3), `StopTerminalSessionRequest` (4).

### tddy-web
- `packages/tddy-web/src/gen/connection_pb.ts` — regenerated from updated proto (buf).
- `packages/tddy-web/src/lib/screenId.ts` *(new)* — stable per-tab screen id from sessionStorage.
- `packages/tddy-web/src/components/sessions/terminalControlState.ts` *(new, stub)* — pure
  reducer for `TerminalControlState` (stubs `applyTerminalControlEvent`).
- `packages/tddy-web/src/components/sessions/useTerminalControl.ts` *(new, stub)* — hook that
  calls `claimTerminalControl` + `watchTerminalControl`; stubs return `isController: false`.
- `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` — `terminalControl` prop added;
  no overlay rendered yet (FIXME stub).
- `packages/tddy-web/src/components/sessions/SessionsDrawerScreen.tsx` — owns
  `useTerminalControl`; passes `terminalControl` to `SessionMainPane`.
- `packages/tddy-web/cypress/support/testIds.ts` — `terminalControlOverlay`,
  `terminalClaimBtn`, `terminalControlHolder` test ids.
- `packages/tddy-web/cypress/support/pages/sessionsDrawerPage.ts` — `terminalControlOverlay`,
  `terminalClaimBtn`, `terminalControlHolder` page-object helpers.

### tddy-daemon
- `packages/tddy-daemon/src/claude_cli_session.rs` — `ClaimOutcome`, `ControlLeaseInfo`,
  `ControlChangeEvent` types; `control` + `control_tx` fields on `ClaudeCliSessionManager`;
  stub methods `claim_control`, `verify_control`, `current_control`, `subscribe_control`.
- `packages/tddy-daemon/src/connection_service.rs` — `MpscControlEventStream` type; stub
  handlers for `claim_terminal_control` + `watch_terminal_control`; updated proto imports.

### Tests (red phase — all expected to fail)
- `packages/tddy-daemon/tests/terminal_control_acceptance.rs` *(new)* — 10 tests
- `packages/tddy-web/cypress/component/TerminalControlAcceptance.cy.tsx` *(new)* — 4 tests
- `packages/tddy-web/src/components/sessions/terminalControlState.test.ts` *(new)* — 5 tests
- `packages/tddy-web/src/lib/screenId.test.ts` *(new)* — 4 tests

### Docs
- `docs/ft/daemon/terminal-sessions.md` — extended with §"Terminal Control Ownership"
- `docs/ft/web/session-drawer.md` — extended with §"Terminal Control — Claim terminal CTA"
