# Screen Sharing Sessions — Product Requirements

**Product area:** Session inspector / remote desktop  
**Feature slug:** `screen-sharing-sessions`  
**Supersedes:** `vnc-sessions` (VNC-specific; this doc generalises to VNC + RDP)

## Problem

When a tddy session is running code inside a VM or remote machine that has a graphical
interface, there is no way to see or interact with that desktop from the tddy web UI. Users
must context-switch to a separate VNC viewer or RDP client, breaking their flow.

## Solution

Attach one or more **screen-sharing targets** to a tddy session. Each target has a label,
host, port, protocol (VNC or RDP), and optional password. From the session inspector's new
**Screen Sharing tab**, the user can add/remove targets, start streaming a target's desktop
into the browser, and close the overlay. While the overlay is open, the browser forwards
mouse and keyboard events back to the remote desktop, providing full remote control.

A single generalized `ScreenSharingService` RPC surface covers both protocols. The protocol
choice (VNC or RDP) is set when adding a target and stored alongside the credentials. The
daemon dispatches to the appropriate bridge binary (`tddy-vnc` or `tddy-rdp`) at stream
start time.

## User stories

- As a user I can add a screen-sharing target (label, host:port, protocol, password) to a
  tddy session so it is remembered for the session's lifetime.
- As a user I am prompted for a passphrase the first time I add a target with a password or
  start a stream, so my passwords are stored encrypted.
- As a user I can see all configured targets in the inspector's Screen Sharing tab, with
  per-target streaming status.
- As a user I can start a stream for a target; the desktop appears as a full-screen overlay
  inside the tddy browser window.
- As a user I can move the mouse and type keys inside the overlay and those events control
  the remote desktop in real time.
- As a user I can close the overlay, which stops the stream and releases resources.
- As a user I can remove a target and its credentials are deleted from the session dir.
- As a user adding a VNC target, the port field defaults to 5900.
- As a user adding an RDP target, the port field defaults to 3389.

## Acceptance criteria

### AC-SS-1: Screen Sharing tab in session inspector

The session inspector drawer shows a **Screen Sharing** tab alongside Details and Tools. The
tab is accessible regardless of whether the session is connected.

### AC-SS-2: Add screen-sharing target

The Screen Sharing tab shows an Add form (label, host, port, protocol selector, password).
The protocol selector offers **VNC** and **RDP**; selecting one updates the default port
placeholder (VNC → 5900, RDP → 3389). Submitting the form:
1. If the vault is locked, the passphrase dialog is shown first.
2. On passphrase confirmation, the vault is created/unlocked, the target is added, and
   appears in the target list.

### AC-SS-3: Protocol selector in the Add form

The Add form includes a protocol selector. Selecting VNC shows port placeholder 5900;
selecting RDP shows port placeholder 3389. The selected protocol is sent as the `protocol`
field in `AddTargetRequest` (`Protocol.VNC` or `Protocol.RDP`). The target list row
displays the protocol label for each target.

### AC-SS-4: Passphrase prompt on first use

The first operation that requires the vault (Add with password, Start stream) shows a
`ScreenSharingPassphraseDialog`. After the user enters a correct passphrase, the vault is
unlocked for the rest of the session. If the vault does not exist yet, the passphrase
creates it.

### AC-SS-5: Start stream

Clicking Start on a target:
1. Calls `StartStream` on `ScreenSharingService`.
2. The daemon spawns the appropriate bridge binary (`tddy-vnc` for VNC targets, `tddy-rdp`
   for RDP targets), which connects to the remote desktop server and publishes a video track
   to the session's LiveKit room. The bridge identity uses the prefix `screenshare-` and the
   track name uses the prefix `screenshare:`.
3. The browser subscribes to the bridge's video track and renders it as a full-screen overlay.

### AC-SS-6: Overlay remote control

While the overlay is open, mouse pointer events (move, click, scroll) and keyboard events
are forwarded over a LiveKit bidi stream (`ScreenSharingInputService.StreamInput`) to the
bridge, which translates and injects them into the remote desktop session using the
protocol-appropriate input mechanism (RFB for VNC, fast-path input for RDP).

### AC-SS-7: Close overlay / stop stream

Closing the overlay calls `StopStream`. The daemon terminates the bridge process and the
video track is unpublished.

### AC-SS-8: Credentials at rest

Passwords are stored encrypted (Argon2 + ChaCha20-Poly1305) in `.screen-sharing.yaml`
inside the session directory (mode 0600). The derived key is cached in daemon memory and
never written to disk. Each target stores its protocol alongside the encrypted password.

### AC-SS-9: On-demand streaming

The bridge process runs only while the overlay is open. Starting two targets spawns two
bridge processes. Stopping removes the child process.

## Out of scope (MVP)

- Audio forwarding
- Multi-monitor (always the primary display)
- Target persistence across sessions (targets are per-session)
- Connection health / reconnect UX beyond initial error display
- NLA / Kerberos authentication for RDP (password auth only for MVP)
