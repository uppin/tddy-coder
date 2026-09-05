# PRD: gate media and presence surfaces on connection capability

**Stack:** `optional-livekit` — node 4 of 7 (`capability-gating`)
**Target PRD on wrap:** [`docs/ft/web/vnc-sessions.md`](../../ft/web/vnc-sessions.md),
[`docs/ft/web/screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md),
[`docs/ft/web/livekit-rooms-panel.md`](../../ft/web/livekit-rooms-panel.md)
**Date:** 2026-09-05

## Problem

Nodes 1–3 made RPC transport-neutral. What is left is everything that is *not* RPC: LiveKit
**tracks** and **presence**. A frame pipe cannot carry a video track, so these surfaces cannot be
abstracted — they have to be absent when the connection cannot serve them.

Today they are unconditional. On a host reached without LiveKit they would render a VNC tab that
never paints, a screen-sharing overlay with nothing to subscribe to, a participant list that is
permanently empty, and a "Sessions" reconciliation that silently loses every cross-host row —
because that reconciliation is itself derived from presence.

The connection already knows the answer: node 1 put `capabilities` on `HostConnection`, node 3 put
it on `SessionConnection`. Nothing consults it yet.

## What this PR delivers

Every media and presence surface renders only when the connection in scope advertises the capability
it needs, and says why when it does not.

### Acceptance criteria

1. `useHasCapability(connection, capability)` is the single predicate; no surface re-derives
   capability from a transport, a status string, or the presence of a `Room`.
2. **Media-gated** (`"media"`), hidden when absent: the VNC tab and `VncOverlay`, the screen-sharing
   tab and `ScreenSharingOverlay`, `ParticipantVideoPreviewDialog`, and the camera-video hooks
   (`hooks/participantCameraVideo.ts`).
3. **Presence-gated** (`"presence"`), hidden when absent: `ParticipantList`, `LiveKitRoomsPanel`,
   `LiveKitAppPage` (and its nav entry), and the participant list in `RpcPlaygroundAppPage`.
4. A gated surface is **removed from navigation**, not rendered disabled — a tab the user cannot use
   is worse than a tab that is not there. Where an entry point must stay visible for layout reasons,
   it carries an explicit "not available on this connection" explanation naming the reason.
5. `SessionsDrawerScreen`'s cross-host reconciliation degrades honestly: without `"presence"` the
   drawer shows what `ListSessions` reports and does not claim the list is complete. The existing
   behaviour with presence is unchanged.
6. `sessionPaneIsWorkflowView` / `attachClaim` decisions are unaffected — gating is about rendering,
   not about attaching.
7. With a `{"rpc"}`-only connection selected, the app has **no** `livekit-client` `Room` constructed
   anywhere in the tree, and no unhandled error or empty-state flash on any screen.
8. With a fully capable LiveKit connection, every surface behaves exactly as it does today; the
   existing Cypress specs for VNC, screen sharing, participants and the rooms panel pass unchanged.

### Non-goals

Merging the terminal components, the IPC transport, desktop registration. See the changeset's
`## Boundaries`.

## Why this shape

- **Hide, don't disable.** A disabled VNC tab invites a support question with no good answer. An
  absent one matches the user's mental model: this host is reached a way that has no video.
- **One predicate.** Three nodes have now added capability information; a fourth place that
  re-derives it from a `Room` would be the drift that undoes the whole stack.
- **Presence is a capability, not a detail.** Cross-host session reconciliation reads as a data
  feature but is built on participants. Naming it as presence-gated is what stops it from silently
  degrading into "this host has no sessions".

## Constraints

- **Zero new npm dependencies** (no public npm registry; `bun run local-registry-install`).
- `tddy-web` only.
- CI does not run Cypress e2e (`docs/dev/guides/ci.md`), so the `{"rpc"}`-only walkthrough needs
  component-level coverage rather than an e2e-only proof.

## Successor PRs

- `feature/optional-livekit/terminal-convergence` — one terminal fed by the session connection.
- `feature/optional-livekit/desktop-ipc-host` — the desktop's own host over IPC.
