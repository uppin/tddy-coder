# Remote desktop input forwarding — Product Requirements

**Date:** 2026-09-07
**Stack:** `#hosts-screen` 9/9 — the top node
**Branch:** `feature/hosts-screen/input-forwarding` → base `feature/hosts-screen/desktop-connect`

## Problem

A remote desktop opened in tddy — from a session's inspector or from a host row — shows a picture
and accepts nothing. An operator can watch a machine and cannot touch it, which for most of the
reasons anyone opens a desktop is the whole task.

This is **not** an unbuilt feature. Everything below the browser works:

| Layer | State |
|---|---|
| `ScreenSharingInputService` (`screen_sharing_input.proto`) — bidi `StreamInput` | defined |
| Bridge serves it over its LiveKit data channel | implemented — `packages/tddy-screenshare/src/bridge.rs` |
| Pump loop → `inject_pointer` / `inject_key` on the protocol client | implemented — `packages/tddy-screenshare/src/client.rs`, both VNC and RDP |
| Generated browser client `src/gen/screen_sharing_input_pb.ts` | generated, **imported nowhere** |
| Capture in `ScreenSharingOverlay` | **absent** |

One missing client, on one surface, for a service that is already listening.

## Solution

Capture pointer and keyboard events on the overlay's video element, translate them into the
neutral values `ScreenSharingInputEvent` carries, and drive `StreamInput` against the bridge
participant over the data channel.

Because both scopes render through the same `ScreenSharingOverlay`, doing this once fixes the
**per-session** desktop and the **host** desktop together.

## User stories

- As an operator watching a host's desktop, I click and type and the remote machine responds, so I
  can fix the thing I opened the desktop to fix.
- As an operator on a session's screen-sharing tab, the same, because it is the same overlay.
- As an operator whose desktop is view-only for a reason the wire imposes, I am told, rather than
  clicking into a surface that silently ignores me.

## Acceptance criteria

- [ ] **AC-IF-1** Pointer movement, button press and release reach the remote desktop.
- [ ] **AC-IF-2** Key press and release reach the remote desktop, including the special keys
      `vncInput.ts` already maps (Enter, Escape, Tab, arrows, function keys).
- [ ] **AC-IF-3** Pointer coordinates are scaled from the rendered video element to the framebuffer,
      so a click lands where the operator aimed at any window size.
- [ ] **AC-IF-4** Input works on a **host-scoped** desktop.
- [ ] **AC-IF-5** Input works on a **session-scoped** desktop — the pre-existing path, unregressed.
- [ ] **AC-IF-6** Escape still closes the overlay rather than being forwarded, and the close button
      and backdrop click still work.
- [ ] **AC-IF-7** The stream is opened when the overlay mounts and closed when it unmounts; no
      stream outlives its overlay.
- [ ] **AC-IF-8** A failure to open the input stream leaves the picture working and says input is
      unavailable, rather than blanking a desktop the operator can still watch.

## Out of scope

- Audio, clipboard, file transfer, multi-monitor.
- Any change to the bridge, the pump loop or the injection path — they work.
- Touch and pen input; pointer and keyboard only.
- Retiring the superseded `vnc_*` surface (see the technical debt note in the changeset).

## Escape is the interesting case

Escape currently dismisses the overlay, and it is also a key a remote desktop legitimately needs.
Forwarding everything traps the operator; forwarding nothing makes the desktop useless. The
resolution is a product decision this node must make explicitly rather than by accident — it is
called out in the changeset's open questions.
