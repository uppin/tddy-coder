# 2026-09-07 — A host's desktop, opened from the Hosts screen
**Type:** Feature

Spans `tddy-service` (host-scoped RPCs and the desktop-password prompt kind), `tddy-daemon` (the
host-scoped target store and the start/stop handlers) and `tddy-web` (the connect action, the
host-scoped overlay and its password dialog).

**What is new is host scope, not remote desktops.** Every `ScreenSharingService` request used to
carry a `session_id`, and the credential vault lives under the session directory. A desktop belongs
to a machine, so `ScreenSharingService` gains `ListHostTargets`, `AddHostTarget`, `StartHostStream`
and `StopHostStream`, addressed by `daemon_instance_id` + `target_id`, and
`host_desktop_targets.rs` keeps those targets in `host-desktop-targets.json` in the host-registry
directory. The bridges, the JSON-on-stdin bridge configuration, the LiveKit republishing and the
browser overlay are reused unchanged; there is no browser-side VNC/RDP protocol client and there
must never be one.

**Two stores, and they cannot see each other.** A host-scoped target is invisible to the session
vault and a session-scoped one is invisible to the host store, so deleting a session leaves a
machine's desktops intact. The host store holds no credential — label, host, port, protocol,
username — so it is published with plain `tddy_core::atomic_file` rather than copying the vault's
hand-rolled writer; a file that exists and does not parse is an error rather than an empty set,
because the callers that go on to write would otherwise replace a damaged file with a file holding
one target.

**The desktop password is prompted, never stored.** `StartHostStream` settles everything that can
fail without a secret first, then raises a `DESKTOP_PASSWORD` prompt on the host prompt channel
stamped with the GitHub user resolved from the caller's session token — so the question reaches that
operator's browser and nobody else can spend its one answer. The browser encrypts under the key the
host publishes with the prompt; the daemon decrypts with the host keypair, hands the plaintext to
the bridge on stdin and drops it. Nothing persists it and it never appears in argv. This
deliberately follows the Hosts screen's prompt-and-drop posture rather than the per-session vault's
storing one. Every start asks, because nothing records whether a desktop wants a password; an empty
answer opens a password-less desktop, and an unanswered prompt fails the start with
`DeadlineExceeded` having spawned no bridge. The wait is bounded by the prompt's own expiry, so an
operator who walks away releases the call the moment the question stops being answerable.

**Host scope has its own room and identity.** A host has no session metadata to take a room from, so
a bridge publishes into the daemon's configured `livekit.common_room` — the room a browser on this
screen already holds a token for — and a daemon with no LiveKit configuration refuses the start.
Bridge identity is `screenshare-host-{instance_id}-{target_id}`, because every host's bridge lands
in that one room. Unlike the deliberately silent session-scoped spawn, a host-scoped spawn failure
is reported: these coordinates tell a browser to mount an overlay, and an operator who just typed a
secret must not be left watching a room nothing joined. Reopening terminates the bridge the previous
open left.

**The action is gated on `media`, and removed rather than disabled.** A frame pipe carries no video,
so on such a host a track cannot arrive at all. Every host-scoped call carries the operator's
session token explicitly — the auth-gated transport rewrites the field only where a request already
carries one, so an omitted token is rejected on arrival rather than repaired in flight.

⚠ **Shipped view-only.** The daemon and the bridge implement input forwarding in full
(`packages/tddy-screenshare/src/bridge.rs` serves `ScreenSharingInputService` over the LiveKit data
channel and its pump loop calls `inject_pointer` / `inject_key`), but the browser has no client —
`packages/tddy-web/src/gen/screen_sharing_input_pb.ts` is imported nowhere — so no overlay sends any
input, on either scope. Tracked at
[`docs/dev/todo/2026-09-07-remote-desktop-input-forwarding.md`](../todo/2026-09-07-remote-desktop-input-forwarding.md).

Top node of the `#hosts-screen` stack — [PR #460](https://github.com/uppin/tddy-coder/pull/460).

See [`docs/ft/web/screen-sharing-sessions.md`](../../ft/web/screen-sharing-sessions.md),
[`packages/tddy-daemon/docs/host-registry.md`](../../../packages/tddy-daemon/docs/host-registry.md)
and [`packages/tddy-web/docs/hosts-screen.md`](../../../packages/tddy-web/docs/hosts-screen.md).
