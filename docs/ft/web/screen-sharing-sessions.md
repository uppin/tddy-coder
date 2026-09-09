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
host, port, protocol (VNC or RDP), and optional password. From the session inspector's
**Screen Sharing tab**, the user can add/remove targets, start streaming a target's desktop
into the browser, close the overlay, and control the desktop from it — the browser forwards mouse
and keyboard events back to the remote machine, specified in
[AC-SS-6](#ac-ss-6-overlay-remote-control).

A single generalized `ScreenSharingService` RPC surface covers both protocols. The protocol
choice (VNC or RDP) is set when adding a target and stored alongside the credentials. The
daemon dispatches to the appropriate bridge binary (`tddy-vnc` or `tddy-rdp`) at stream
start time.

A desktop can belong to a **host** instead of to a session. A machine outlives any piece of work
done on it, so a host's desktop is attached to the host itself and opened straight from the Hosts
screen without creating a session first. Both scopes share the same bridges, the same LiveKit track
and the same overlay — see [Two scopes](#two-scopes-a-sessions-desktop-and-a-hosts).

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
  the remote desktop in real time ([AC-SS-6](#ac-ss-6-overlay-remote-control)).
- As a user I can close the overlay, which stops the stream and releases resources.
- As a user I can remove a target and its credentials are deleted from the session dir.
- As a user adding a VNC target, the port field defaults to 5900.
- As a user adding an RDP target, the port field defaults to 3389.
- As an operator I can open a host's desktop from its row on the Hosts screen, without creating a
  session for it first.
- As an operator I am asked for that desktop's password each time I open it, and it is kept nowhere.
- As an operator I do not see a connect action on a host tddy could not stream from, so I am never
  offered a control that cannot work.

## Acceptance criteria

### AC-SS-1: Screen Sharing tab in session inspector

The session inspector drawer shows a **Screen Sharing** tab alongside Details and Tools, on a host
reached over a wire that carries video. The tab is accessible regardless of whether the session is
connected — a dormant session's targets are still worth reading — but not on a host whose
connection carries no media (see [Availability](#availability)).

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

**Specified:** while the overlay is open, mouse pointer events (move, click, scroll) and keyboard
events are forwarded over a LiveKit bidi stream (`ScreenSharingInputService.StreamInput`) to the
bridge, which translates and injects them into the remote desktop session using the
protocol-appropriate input mechanism (RFB for VNC, fast-path input for RDP).

**Delivered on both scopes**, by the one overlay both of them mount. The browser opens the stream
when the overlay mounts and closes it when the overlay goes away; pointer positions are scaled from
the rendered picture into framebuffer pixels on every event, so a click lands where the operator
aimed at any window size, and keys are forwarded as neutral X11 keysyms.

**Escape goes to the desktop**, and `Ctrl+Alt+Esc` is the only chord the overlay keeps for itself —
named on screen, because every other key leaves the browser. Swallowing Escape would put every
full-screen application on the far side out of reach; forwarding everything with no keyboard exit
would trap the operator. Browser-reserved combinations (`Cmd+W`, `F5`, `Cmd+Tab`) cannot be captured
by a page at all and never reach the desktop.

**Whatever the operator is still holding is released when the overlay closes.** Otherwise the chord
itself would leave Ctrl and Alt down on the remote machine, and every later keystroke there would
arrive as a Ctrl+Alt chord.

A desktop whose input stream cannot be opened keeps its picture and says input is unavailable, so it
can still be watched. Note what that notice does *not* cover: a bridge that cannot inject drops
commands silently rather than refusing the stream, so a genuinely view-only server is ignored, not
reported.

Not forwarded: scroll wheel, audio, clipboard and file transfer. See
[`packages/tddy-web/docs/remote-desktop-input.md`](../../../packages/tddy-web/docs/remote-desktop-input.md)
for how the client is built.

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

### AC-SS-10: Connect action on a host row

A host whose desktop is reachable, whose daemon has the bridge binary for that protocol, and whose
connection carries media offers a **connect** action in its Hosts row's remote-desktop section.

### AC-SS-11: Opening a host's desktop

Connecting opens the same full-screen overlay on that host's desktop, streaming the bridge's video
track. The overlay opens on the click rather than on the reply, so the interval a remote spawn takes
is visible and a failure has somewhere to be reported.

### AC-SS-12: Closing a host desktop

Closing the overlay stops the host-scoped stream and releases the bridge process. Reopening a
desktop terminates any bridge the previous open left running rather than orphaning it.

### AC-SS-13: No media, no action

A host reached over a connection that does not carry media offers **no connect action at all** — the
control is absent, not disabled.

### AC-SS-14: Nothing to connect to, no action

A host whose desktop the probe found unreachable, or whose daemon has no bridge binary, offers no
connect action either.

### AC-SS-15: The desktop password is asked for, never stored

Opening a host's desktop asks the operator for that desktop's password on the host's encrypted
prompt channel, uses it once and keeps it nowhere — not on disk, not in daemon state past the call,
and never in a process argument. An empty answer is a valid one and opens a password-less desktop.
A question nobody answers before it expires fails the start and spawns no bridge.

### AC-SS-16: The two scopes are separate stores

A host-scoped target is not visible to the session-scoped store and a session-scoped target is not
visible to the host-scoped one. Deleting a session leaves the host's desktops intact.

## Availability

The picture is a published video track, so this feature exists only where the host is reached over a
wire that carries tracks. On a host whose connection does not, the **Screen Sharing** tab is
**absent from the inspector's tab strip** rather than present and disabled, and a
`?inspector=screen-sharing` deep link degrades to Details — honoured again the moment the wire can
serve it.

The gate is the **host's** connection, not the session's: a dormant session has no session
connection to ask, and whether a session is carried over a room is decided by how its host is
reached in the first place. A join that is merely still in flight, or one that failed, keeps the tab
— the reason a join failed is what an operator opens the screen to find.

Only the **picture** needs a media wire; the input half (`ScreenSharingInputService.StreamInput`) is
ordinary RPC that any transport could carry. The whole tab is gated regardless, because a remote
desktop with input and no picture is not a feature.

See [capability gating](../../../packages/tddy-web/docs/capability-gating.md) for the rule all the
media and presence surfaces share.

## Two scopes: a session's desktop, and a host's

A screen-sharing target belongs either to a **session** or to a **host**. Which one it is decides
where it is added from, where it is stored, and what happens to its credential — and nothing else.

| | Session-scoped | Host-scoped |
|---|---|---|
| Belongs to | one tddy session | one machine in the host registry |
| Opened from | the session inspector's Screen Sharing tab | the remote-desktop section of that host's row on the Hosts screen |
| Stored in | `.screen-sharing.yaml` in the session directory | `host-desktop-targets.json` in the daemon's host-registry directory |
| Credential | stored, encrypted, unlocked with the vault passphrase | asked for on every open and kept nowhere |
| Outlives | nothing — deleting the session deletes it | every session; nothing a session does removes it |

Below the addressing the two are the same feature: the same `tddy-vnc` / `tddy-rdp` bridge binaries,
the same JSON-on-stdin configuration so no credential reaches a command line, the same LiveKit video
track, and the same full-screen overlay. A desktop belongs to a machine, so entangling the two
scopes would make deleting a piece of work delete a machine's desktop.

**Rendering is always a daemon-produced LiveKit video track.** There is no browser-side VNC or RDP
protocol client on either scope, and there must never be one: the picture an operator sees is
published by a bridge process the daemon spawned. This is a permanent architectural boundary, not a
stage of the work.

### Connecting from a host row

The connect action appears on a host row when three facts hold at once: the probe found something
serving a desktop, that host's daemon has the bridge binary for the protocol serving it, and the
host is reached over a connection that **carries media**. The first two are the row's own reading
(see [`hosts-screen-tooling.md`](./hosts-screen-tooling.md)); the third is the ordinary media gate
every track surface answers to.

**Without media the action is absent, not disabled.** A frame pipe carries no video, so on a host
reached that way a LiveKit track cannot arrive however reachable its desktop is, and a control that
provably cannot work is worse than no control. See
[capability gating](../../../packages/tddy-web/docs/capability-gating.md).

A host's bridge publishes into the daemon's configured common room — a session has a room in its
metadata and a host has none, and the common room is the one the Hosts screen already holds a token
for. Each bridge is identified by the host and the target together, because every host's bridge
lands in that same room. A daemon with no LiveKit configuration to reach refuses the start rather
than returning coordinates nothing can join.

Every host-scoped call carries the operator's own session token, and the daemon rejects one that
does not — so a host's desktops are reachable only by an authenticated operator, on every wire.

### The desktop password is asked for, and kept nowhere

A host desktop's password is **prompted, never stored**, which is deliberately unlike the
session-scoped vault:

1. The daemon raises the question on that host's encrypted prompt channel, stamped with the GitHub
   user whose call is blocked on it. Nobody else is shown the question and nobody else can spend its
   one answer.
2. The browser encrypts the answer under the public key the host published with the question, having
   first checked that key against the one it pinned for this host.
3. The daemon decrypts it, hands it to the bridge on the bridge's **stdin**, and drops it. It is
   never written to disk, never held in daemon state past the call, and never appears in a process
   argument.

The daemon asks on **every** open, because nothing records whether a given desktop wants a password
and a daemon that guessed would either skip the question for a desktop that needs one or lock out
one that does not. An empty answer is therefore a real answer, and is how a password-less desktop is
opened. A question nobody answers before it expires fails the start — the call returns
`DeadlineExceeded` and no bridge is spawned, rather than a process left authenticating to nothing.

Why not the vault. The session-scoped vault exists and stores a credential, so this is two postures
in one product. The Hosts screen is the deciding context: everything else it does with a secret —
loading an ssh key, for one — prompts and drops, and one model across that screen is easier to
reason about than two. A host also has no session directory to keep an encrypted file in, and the
prompt channel is the only place a host publishes the key an answer could be encrypted under.

### What the Hosts screen reports, and what it does

Whether a host has a desktop to reach at all is answered per host and per protocol, on the default
ports, as two distinct facts: whether that host's daemon can spawn a bridge, and whether anything is
serving a desktop there. That **reporting** starts no stream, spawns no bridge and touches no
session target or vault — it is a bare TCP connect plus a check that a binary exists. **Connecting**
is the separate action described above, offered on the same row where those facts are reported, and
it is the only thing on that screen that creates a host-scoped target or starts a bridge. See
[`hosts-screen-tooling.md`](./hosts-screen-tooling.md).

## Current limits

- **A remote desktop cannot be scrolled.** Pointer and keyboard are forwarded
  ([AC-SS-6](#ac-ss-6-overlay-remote-control)); the wheel is not, on either scope. RFB carries scroll
  as buttons 4 and 5, so `button_mask` could already express it — nothing but a `wheel` listener is
  missing.
- A bridge process runs per open desktop. That is the same resource profile on both scopes, and on
  the host scope it is reachable from a screen that lists every host at once.
- Only the default ports are probed, so a host serving on another display is not offered a connect
  action even though a target pointed at that port would work.

## Out of scope (MVP)

- Audio forwarding
- Multi-monitor (always the primary display)
- Session-scoped targets do not persist across sessions; a desktop meant to outlive one belongs to
  its host instead
- Connection health / reconnect UX beyond initial error display
- NLA / Kerberos authentication for RDP (password auth only for MVP)
- Discovering non-default desktop ports
