# 2026-09-06 — LiveKit can be turned off without deleting its configuration

**Type:** Feature

Node 8 of the `optional-livekit` stack ([#449](https://github.com/uppin/tddy-coder/pull/449)).
Nodes 1–7 made every surface work when LiveKit is *absent*. They did nothing for the operator who
has it configured and wants it **off** — for a machine that should not appear in the common room, a
laptop that should not hold a media connection, or a dev run where the room is noise. Until now the
only way to express that was to delete a working `livekit:` block, so "off" cost the operator their
url, key, secret and room name.

**`livekit.enabled` is that switch, and it defaults to `false`.** This is a deliberate change of
default and the most consequential decision here: **an existing deployment with a working `livekit:`
block stops joining the common room on upgrade** until someone adds `enabled: true`. It is the right
default for a stack whose premise is that LiveKit is optional, and it fails safe — a daemon that has
not been told to join does not join — but it is a breaking change. Anything in this repository that
renders a `livekit:` block now says so explicitly: `dev.daemon.yaml`, `dev.desktop.yaml`, and
`tddy-vm`'s baked guest config, which would otherwise have produced a VM holding every credential and
joining nothing, discoverable only hours into a bake.

Every common-room decision routes through **one** predicate, `LiveKitConfig::common_room_enabled`,
rather than a ninth re-derivation of "is LiveKit usable" — discovery found eight existing ones, each
with a slightly different field set, and a flag bolted onto one of them would have shipped half
applied. Switched off: `CommonRoomTarget::from_livekit` yields no target so the supervisor leaves the
room and stays out; `livekit_common_room_connect_strings` refuses, which is what stops peer discovery
assembling and the advertisement publishing; and both mints refuse a common-room token —
`MintLiveKitToken` outright, and `token.TokenService` for that one named room, so per-session rooms
and screen sharing keep working. The supervisor logs *disabled* at info where it logs *incomplete* at
warning: a daemon told not to join has not been misconfigured, and the two must not read alike.

**The switch never disarms authentication.** `livekit.api_secret` is also the session-token signing
secret, so reading "disabled" as "the block is absent" would leave the daemon with no signer and
refuse every gated RPC — including `DaemonConfigService`, the one an operator switches LiveKit back
on from. That lockout is the single constraint the whole design is shaped around, and it is pinned by
a test rather than left to review. For the same reason `token.TokenService` authenticates *before* it
looks at the requested room, so a refusal never doubles as an inventory of which rooms a deployment
has switched off.

Saving the toggle takes effect live: the reconnect predicate keys on the flag as well as `(url,
common_room)`, so switching off disconnects a running room and switching on rejoins it, neither
needing a restart. The daemon reports the state to the page it serves (`GetClientConfig`,
`/api/config`), and the web app short-circuits on it — no token minted, no `Room` constructed, and the
host directory reporting the LiveKit source as `idle`, never `error`. A deliberate "off" and a
never-configured deployment reach the same quiet outcome by different routes, which is the rule node 2
established. The Settings screen renders the switch as a checkbox above the LiveKit fields.

Feature docs [daemon-settings.md](../../ft/daemon/daemon-settings.md),
[livekit-peer-discovery.md](../../ft/daemon/livekit-peer-discovery.md); technical
[host-directory.md](../../../packages/tddy-web/docs/host-directory.md).
(tddy-daemon, tddy-coder, tddy-service, tddy-vm, tddy-web)
