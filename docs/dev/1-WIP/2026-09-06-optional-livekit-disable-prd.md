# PRD: LiveKit can be turned off without deleting its configuration

**Stack:** `optional-livekit` — node 8 of 8.
Changeset: [`2026-09-06-optional-livekit-disable.md`](2026-09-06-optional-livekit-disable.md)
Discovery: [`2026-09-06-optional-livekit-disable-initial-discovery.md`](2026-09-06-optional-livekit-disable-initial-discovery.md)

## Problem

Nodes 1–7 made the app work when LiveKit is *absent*. They did nothing for the operator who has it
configured and wants it **off** — for a machine that should not appear in the common room, a laptop
that should not hold a media connection, or a dev run where the room is noise.

The only way to turn it off today is to delete a working `livekit:` block, which means keeping the
url, key, secret and room somewhere else and pasting them back to turn it on again. "Off" is
expressible only as data loss.

It is also invisible. A config that has LiveKit and one that never did are indistinguishable in the
UI, so an operator staring at a host that is not appearing cannot tell which they are looking at.

## What this PR delivers

A `livekit.enabled` flag in the daemon's YAML, surfaced as a toggle on the existing Settings screen.
Everything else in the block is preserved verbatim; turning it back on is one toggle.

**`enabled` defaults to `false`, so LiveKit is opt-in.** This is a deliberate change of default and
the most consequential decision here: **an existing deployment with a working `livekit:` block stops
joining the common room on upgrade** until someone adds `enabled: true`. That includes this repo's own
`dev.desktop.yaml`. It is the right default for a stack whose premise is that LiveKit is optional, and
it fails safe — a daemon that has not been told to join does not join — but it is a breaking change
for anyone relying on the old behaviour, and the changelog must say so.

### Acceptance criteria

1. `livekit.enabled` exists in `LiveKitConfig`, defaults to `false`, and a config that omits the key
   reads as disabled. A config that never had a `livekit:` block at all behaves exactly as it does
   today.
2. Disabled, the daemon **joins no common room**: `CommonRoomTarget::from_livekit` yields no target,
   peer discovery is not assembled, and the supervisor logs that it is disabled rather than that the
   block is incomplete — the two are different operator problems and must read differently.
3. Disabled, the daemon **publishes no peer metadata** and no advertisement.
4. Disabled, the daemon **mints no common-room tokens** — neither `TokenService` nor
   `MintLiveKitToken` hands one out.
5. **The API secret keeps signing session tokens.** Disabling LiveKit must not disturb
   `auth.rs:63`'s `signing_secret`, or every gated daemon RPC would refuse — including
   `DaemonConfigService`, locking the operator out of re-enabling it. Pinned by a test that a gated
   RPC still authenticates with LiveKit disabled.
6. `GetClientConfig` reports the disabled state, and the web app **constructs no `Room` and mints no
   token** — the same guarantee an unconfigured deployment already has, reached by a different route.
7. The host directory reports the LiveKit source as **`idle`, never `error`**, when disabled. An
   operator who turned it off is not shown a failure.
8. The Settings screen has a toggle. Saving it **disconnects a live common room** — i.e. the reconnect
   predicate accounts for the flag, not only for `(url, common_room)`.
9. Re-enabling reconnects without a restart, and the url, key, secret and room are unchanged in the
   YAML throughout.
10. Everything unrelated to the common room keeps working while disabled: per-session rooms, screen
    sharing and the `#/livekit` rooms panel read the same block for their own purposes and are not
    governed by this flag.

### Non-goals

- Disabling LiveKit *per host* — this is the serving daemon's own setting.
- A client-side or per-browser override.
- Removing or changing any of the seven existing re-derivations of "is LiveKit usable" beyond routing
  them through one predicate.
- Changing what `enabled: true` does. This node adds an off switch; it does not touch the on path.

## Why this shape

**Daemon *and* client, not one or the other.** If the daemon still joined, peers would keep seeing this
host and it would still hold a LiveKit connection — "disabled" would be a statement about the browser,
not the integration. If only the daemon disabled, the Settings screen would show live-looking config
with no client-visible reason why nothing happens.

**One predicate, not an eighth re-derivation.** Discovery found seven places that each decide "is
LiveKit usable" from a slightly different field set. A flag bolted onto one of them ships half
applied. The flag belongs on the config with a single accessor the join paths delegate to.

**Preserved verbatim.** `apply_update` already clones the whole `DaemonConfig` and edits two sub-trees,
so unrelated YAML survives an update structurally rather than by convention. The toggle inherits that.

## Constraints

- **No new npm or Rust dependencies.**
- `LiveKitConfig` carries `#[serde(deny_unknown_fields)]`, so the field and the YAML key must land
  together; a downgraded daemon rejects a config a newer UI wrote.
- Proto field numbers are fixed by what is already used: `LiveKitSettings` → **7**,
  `GetClientConfigResponse` → **8**. Never reuse a retired number.
- `tddy-desktop` and Cypress e2e are outside the CI gate, so a green PR proves neither.

## Successor PRs

None — this is the stack's top node.
