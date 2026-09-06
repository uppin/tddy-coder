# PRD — Open a host's desktop in the in-app viewer

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (viewer, action) + `daemon` (host-scoped targets, bridge start)
**Stack:** `#hosts-screen` node 8 of 8 — the top node
**Branch:** `feature/hosts-screen/desktop-connect` → base `feature/hosts-screen/desktop-probe`

## Affected features

| Document | Relationship |
|---|---|
| [`docs/ft/web/screen-sharing-sessions.md`](../screen-sharing-sessions.md) | The per-session feature whose bridge, overlay and input channel this node reuses at host scope |
| [`PRD-2026-09-06-desktop-probe.md`](./PRD-2026-09-06-desktop-probe.md) | Reports the reachability that gates this action |
| [`docs/ft/web/capability-gating.md`](../../../packages/tddy-web/docs/capability-gating.md) | Governs where this action can appear at all |

## Summary

From a host row, **open that host's desktop in tddy-web** — the existing full-screen overlay, streaming
the remote framebuffer as a LiveKit video track, with mouse and keyboard forwarded back.

## Background

The hard parts already exist. `packages/tddy-vnc` and `packages/tddy-rdp` bridge RFB and RDP into
LiveKit video tracks; `ScreenSharingOverlay.tsx` renders such a track full-screen and forwards input.
**There is no browser-side VNC/RDP protocol client, and this node does not add one** — rendering is
always a daemon-produced video track.

What does not exist is **host scope**. Every `ScreenSharingService` request carries a `session_id`, and
the credential vault lives under the session directory. A desktop belongs to a *machine*, not to a
coding session, and today there is no way to say so.

## Proposed changes

### What is changing

- **A host-scoped target model** — a desktop target belonging to a host rather than a session: label,
  host, port, protocol, username.
- **Host-scoped start/stop**, reusing the existing bridge spawn path (config as JSON on the bridge's
  **stdin**, so credentials never appear in argv) and returning the same
  `{ livekit_room, livekit_url, bridge_identity, track_name, width, height }` the overlay already
  consumes.
- **A connect action on the host row**, and the existing overlay mounted at host scope.

### ⚠ This action cannot exist without media

`InspectorTabs.tsx:101-110` **removes** the VNC/Screen Sharing tabs when a connection lacks the `media`
capability, and `IPC_CAPABILITIES` is `{"rpc"}` — a frame pipe carries no video. So on the desktop
build's own IPC-reached host, a LiveKit video track cannot arrive at all.

The connect action is therefore gated on `media` and, following the existing precedent, **removed
rather than shown broken** where media is unavailable. This is a real limit of the feature, stated
plainly rather than discovered.

### The credential decision this PRD must make explicitly

A remote desktop usually needs a password, and the two in-repo precedents disagree:

| Precedent | Posture |
|---|---|
| `screen_sharing_vault.rs` + `UnlockVault` (per session) | **Stores** the credential, encrypted |
| Node 6 (`agent-add-key`) | **Never persists** — prompt, forward encrypted, drop |

**This PRD's position: reuse node 6's encrypted prompt channel and do not persist a host desktop
password.** One secret-handling posture across the Hosts screen is easier to reason about than two, and
node 6 has already built the channel. If a reviewer prefers the vault precedent, that is a legitimate
argument — but it must be made deliberately, not inherited by accident.

### What is staying the same

- **The bridges are untouched** — no change to `packages/tddy-vnc`, `packages/tddy-rdp`, or the spawn
  path in `screen_sharing_service.rs`.
- **The overlay and input channel are reused**, not rewritten.
- **Per-session screen sharing keeps working unchanged**, including its vault and its tabs.
- No browser-side protocol client is introduced.

## Impact analysis

### Technical

- A bridge process per active host desktop, PID-tracked for stop, exactly as the session path does.
- Host-scoped storage for targets, alongside (not inside) the session vault.
- The action is only offered where node 7 reports the desktop reachable **and** the connection carries
  media.

### User

- An operator opens a host's desktop from the Hosts screen without creating a session first.
- On hosts reached over IPC, or any wire without media, the action is absent — with the reason visible
  rather than a broken control.

## Acceptance criteria

- [ ] **AC-1** A host with a reachable desktop and a media-carrying connection offers a connect action.
- [ ] **AC-2** Connecting opens the existing overlay streaming that host's desktop.
- [ ] **AC-3** Mouse and keyboard input reach the remote desktop.
- [ ] **AC-4** Closing the overlay stops the stream and releases the bridge process.
- [ ] **AC-5** A host whose connection lacks `media` does **not** offer the action.
- [ ] **AC-6** A host node 7 reports unreachable does not offer the action.
- [ ] **AC-7** A desktop password is prompted through node 6's encrypted channel and **not persisted**.
- [ ] **AC-8** Host-scoped targets are separate from session-scoped ones; neither leaks into the other.
- [ ] **AC-9** Per-session screen sharing continues to work unchanged.
- [ ] **AC-10** Credentials never appear in process arguments.

## Out of scope for this node

A browser-side VNC/RDP protocol client (explicitly never). Any change to the bridges or the per-session
flow. Discovering non-default ports. Multi-monitor selection.

## This is the top node

Nothing branches off this PR. When `#optional-livekit` lands and node 1 is repointed onto `master`,
this node lands last.
