# VNC Sessions — Product Requirements

**Product area:** Session inspector / remote desktop  
**Feature slug:** `vnc-sessions`

## Problem

When a tddy session is running code inside a VM or remote machine that has a graphical
interface, there is no way to see or interact with that desktop from the tddy web UI. Users
must context-switch to a separate VNC viewer, breaking their flow.

## Solution

Attach one or more **VNC targets** to a tddy session. Each target has a label, host, port,
and optional password. From the session inspector's new **VNC tab**, the user can add/remove
targets, start streaming a target's desktop into the browser, and close the overlay. While
the overlay is open, the browser forwards mouse and keyboard events back to the VNC server,
providing full remote control.

## User stories

- As a user I can add a VNC target (label, host:port, password) to a tddy session so it is
  remembered for the session's lifetime.
- As a user I am prompted for a passphrase the first time I add a target with a password or
  start a stream, so my VNC passwords are stored encrypted.
- As a user I can see all configured VNC targets in the inspector's VNC tab, with per-target
  streaming status.
- As a user I can start a VNC stream for a target; the desktop appears as a full-screen
  overlay inside the tddy browser window.
- As a user I can move the mouse and type keys inside the overlay and those events control
  the remote VNC desktop in real time.
- As a user I can close the overlay, which stops the VNC stream and releases resources.
- As a user I can remove a VNC target and its credentials are deleted from the session dir.

## Acceptance criteria

### AC-VNC-1: VNC tab in session inspector

The session inspector drawer shows a **VNC** tab alongside Details and Tools. The tab is
accessible regardless of whether the session is connected.

### AC-VNC-2: Add VNC target

The VNC tab shows an Add form (label, host, port, password). Submitting it:
1. If the vault is locked, the passphrase dialog is shown first.
2. On passphrase confirmation, the vault is created/unlocked, the target is added, and
   appears in the target list.

### AC-VNC-3: Passphrase prompt on first use

The first operation that requires the vault (Add with password, Start stream) shows a
`VncPassphraseDialog`. After the user enters a correct passphrase, the vault is unlocked for
the rest of the session. If the vault does not exist yet, the passphrase creates it.

### AC-VNC-4: Start stream

Clicking Start on a target:
1. Calls `StartVncStream` on `VncService`.
2. The daemon spawns a `tddy-vnc` bridge binary, which connects to the VNC server and
   publishes a video track to the session's LiveKit room.
3. The browser subscribes to the bridge's video track and renders it as a full-screen overlay.

### AC-VNC-5: Overlay remote control

While the VNC overlay is open, mouse pointer events (move, click, scroll) and keyboard
events are forwarded over a LiveKit bidi stream (`VncInputService.StreamInput`) to the bridge,
which sends them as RFB input events to the VNC server.

### AC-VNC-6: Close overlay / stop stream

Closing the overlay calls `StopVncStream`. The daemon terminates the bridge process and the
video track is unpublished.

### AC-VNC-7: Credentials at rest

VNC passwords are stored encrypted (Argon2 + ChaCha20-Poly1305) in `.vnc.yaml` inside the
session directory (mode 0600). The derived key is cached in daemon memory and never written
to disk.

### AC-VNC-8: On-demand streaming

The bridge process runs only while the overlay is open. Starting two targets spawns two bridge
processes. Stopping removes the child process.

## Out of scope (MVP)

- Audio forwarding
- Multi-monitor (always the primary display)
- VNC target persistence across sessions (targets are per-session)
- Connection health / reconnect UX beyond initial error display
