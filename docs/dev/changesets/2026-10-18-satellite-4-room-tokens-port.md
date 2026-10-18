# 2026-10-18 — Room tokens through an extension port

**Type:** Architecture

`#satellite` 4/13 ([#626](https://github.com/uppin/tddy-coder/pull/626)). Minting a room access token no
longer requires the LiveKit crates in `tddy-screen-sharing`, and `tddy-daemon-auth` builds no token
generator of its own.

## What changed

- **`RoomTokenMinter` port and `NoRoomTokens`** in `tddy-daemon-extensions`.
  [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-room-tokens-port-room_tokens).
- **`room_token_minter(Option<&LiveKitConfig>)`** in `tddy-daemon-livekit`: a minter over
  `TokenGenerator` for a complete block, a refusal naming the missing fields otherwise.
- **`tddy-daemon-auth`.** `MinterTokenProvider` replaces `LiveKitTokenProvider`; `build_auth_entries`,
  `build_auth_entries_with`, `build_auth_entries_admitting`, `build_token_service_entry` and
  `LiveKitTokenServiceImpl::new` take the minter.
- **`tddy-screen-sharing`** mints a bridge's token through `with_room_tokens` and no longer reads the
  key and secret; `tddy-livekit` is a dev-dependency only. It left the ratchet's `KNOWN_OFFENDERS`.
- **`tddy-daemon`** builds the minter once in `runtime::build`.

## Not changed

Minting in `tddy-coder`, `tddy-session-sync`, `tddy-terminal-rpc`, `tddy-cli-sessions`,
`tddy-session-agents` and `tddy-session-split` (later nodes), `SessionTokenMinter`, and the OAuth
loopback tunnel that keeps `tddy-daemon-auth` on the ratchet's list.

## Verification

Scoped to `tddy-daemon-extensions`, `tddy-daemon-livekit`, `tddy-daemon-auth` and
`tddy-screen-sharing`: the null object, the minter decoded with LiveKit's verifier, the token service
over a minter, the bridge's token, and both ratchet tests pass; `cargo clippy -D warnings` and
`cargo check --all-targets` are clean for those and `tddy-daemon`. Three LiveKit-testkit suites were
only compiled; CI runs them.

## Code issues

Grown by the minimum wiring, split deferred with the developer's consent (later `#satellite` nodes touch
the same files): `oversized-file-runtime` 1,813 → 1,820; `oversized-file-screen-sharing-service`
967 → 983; `complexity-runtime-build` 1,047 → 1,054 lines;
`complexity-auth-build-auth-entries-admitting` 110 → 111 lines. `oversized-file-auth` 622 → 619, not
clean; the record stays. Backlog entries `2026-08-13-tokengenerator-generate-for-performs-no-authorization`
and `2026-09-06-from-the-2026-09-06-optional-livekit-stack-evaluation` are touched, not resolved.
