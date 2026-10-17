# Room tokens through an extension port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 4/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Auth and LiveKit services](../auth-livekit-services.md). `token.TokenService`
  and `auth.LiveKitTokenService` mint LiveKit room tokens. They behave the same, but the minting
  goes through a port.
- **Related Feature**: screen sharing (`tddy-screen-sharing`). A desktop-publishing bridge is handed
  a room token minted through the same port.

## Summary

Minting a LiveKit access token for a room and identity goes through a `RoomTokenMinter` extension
port. The LiveKit plugin builds the real minter from the `livekit:` block, and when the block lacks a
credential, the minter's refusal names the missing field. A daemon with no LiveKit gets
`NoRoomTokens`, which refuses with a precondition failure. `tddy-screen-sharing` stops naming
`tddy-livekit` and leaves the ratchet's `KNOWN_OFFENDERS`.

## Background

Token minting is the one piece of LiveKit that `tddy-screen-sharing` uses: it builds a
`tddy_livekit::TokenGenerator` from config fields to hand a bridge subprocess
(`screen_sharing_service.rs:503`). `tddy-daemon-auth` builds the same generator twice, for the token
service (`auth.rs:330`) and for the git-command room token (`auth.rs:523`). Each re-reads the API
key and secret from config, which is one of the "is LiveKit configured?" re-derivations the
`#optional-livekit` evaluation flagged.

## Proposed Changes

### What's Changing
- `RoomTokenMinter` port and its `NoRoomTokens` null object in `tddy-daemon-extensions`.
- `tddy_daemon_livekit::room_tokens::room_token_minter(Option<&LiveKitConfig>) -> Arc<dyn
  RoomTokenMinter>` returns the real minter or a refusal naming what is missing.
- Screen sharing receives the minter and stops reading the key and secret itself.
- `tddy-daemon-auth`'s token service and LiveKit token service mint through an injected minter. It
  adapts the minter onto `tddy_service::TokenProvider`, which stays the token service's own port.
- `tddy-screen-sharing` is struck from the ratchet.

### What's Staying the Same
- Token contents: room, identity, grants, and the TTLs (`DEFAULT_LIVEKIT_JWT_TTL_SECS`,
  `MINTED_ROOM_TOKEN_TTL`).
- `tddy_service::TokenProvider` and `TokenServiceImpl`.
- The minting done by client tools and other daemon subsystems (`tddy-coder`, `tddy-session-sync`,
  `tddy-terminal-rpc`, `tddy-cli-sessions`, `tddy-session-agents`, `tddy-session-split`). Each is
  moved by its own node.
- `tddy-daemon-auth` stays a known offender. Its OAuth loopback tunnel over the common room is
  node 10's.

## Impact Analysis
- **Technical:** the daemon builds one minter from config at startup and hands it to the auth
  builders and screen sharing. Screen sharing's error for a missing credential now comes from the
  minter.
- **User:** none. Error wording for an incomplete `livekit:` block keeps naming the missing field.

## Implementation Plan
1. Add the port and its null object, then the plugin's config-built minter, with tests.
2. Inject the minter into screen sharing and the two daemon-auth mint paths.
3. Trim screen-sharing's manifest and strike its ratchet entry.

## Success Criteria
- `tddy-screen-sharing` names no LiveKit crate, and the ratchet agrees.
- A token minted through the port verifies against the configured secret, for the requested room
  and identity.
- An incomplete block mints nothing and names the missing field.

## Out of Scope
- Other minting sites (nodes 6, 7, 8 and 11) and daemon-auth's room tunnel (node 10).
