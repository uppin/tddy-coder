# Terminal exposure through an extension port - PRD

**Date**: 2026-10-16
**PRD Type**: Technical Improvement
**Stack**: #satellite 6/11. Umbrella PRD on branch `satellite-1-extension-ports` (#623).

## Affected Features

- **Primary Feature**: [Claude CLI session](../claude-cli-session.md). A session's PTY is served to
  remote participants. The behaviour is unchanged, but it is reached through a port.
- **Related Feature**: [Telegram session control](../telegram-session-control.md). A
  Telegram-started session bridges its terminal at once, through the same port.

## Summary

Making a session's terminal drivable from another host goes through a `TerminalExposure` extension
port. The CLI session manager hands the port the terminal's RPC service (the PTY bridged to
`terminal.TerminalService`, built only from `tddy-rpc` types), a room-scoped address, and the
session's metadata. The LiveKit plugin joins the room, mints the participant's token and republishes
the metadata. `tddy-cli-sessions` stops naming `tddy-livekit` and leaves the ratchet's
`KNOWN_OFFENDERS`. With nothing wired, `NoTerminalExposure` exposes nothing, and the session's
terminal stays reachable over the daemon's own connection, as on a desktop today.

## Background

`tddy-cli-sessions/src/cli_session_manager/livekit_bridge.rs` is the crate's only LiveKit code. It
mints a token (`TokenGenerator`), joins as a `LiveKitParticipant`, and spawns
`spawn_local_participant_metadata_watcher`. Its `LiveKitTerminalAddress` carries the API key and
secret through two crates (`terminal_bridge_impl.rs` and telegram's `session_start.rs`) that have no
other reason to hold them.

## Proposed Changes

### What's Changing
- Port `TerminalExposure` with `TerminalAddress { room, identity }`, an `ExposedTerminal` handle that
  reports when the exposure has ended, and the `NoTerminalExposure` null object, in
  `tddy-daemon-extensions`.
- The plugin's `LiveKitTerminalExposure` (URL and credentials from config) holds today's
  `spawn_livekit_bridge` body.
- `CliSessionManager::ensure_livekit_terminal` becomes `ensure_exposed_terminal(session_id, at,
  &dyn TerminalExposure)`, keeping its single-flight behaviour. The PTY RPC service becomes a public,
  LiveKit-free `pty_terminal_service(handle)`.
- `DaemonSessionHost`'s `SessionTerminalBridge` impl and telegram's immediate bridge call the port.
- `tddy-cli-sessions` drops `tddy-livekit` and is struck.

### What's Staying the Same
- Who is bridged and when: lazily on a remote reach, immediately for Telegram.
- The participant identity, the room name, the 24 h token TTL, and the metadata republish interval.
- The single-flight write lock. It still spans the join; see Out of Scope.

## Impact Analysis
- **Technical:** credentials leave `LiveKitTerminalAddress`. The plugin reads them from config once.
- **User:** none.

## Implementation Plan
1. Add the port, the address, the handle and the null object.
2. Make the PTY service public and LiveKit-free, and move the bridge body into the plugin.
3. Switch the session manager, lifecycle's bridge and telegram. Trim the manifest and strike the
   ratchet entry.

## Success Criteria
- `tddy-cli-sessions` names no LiveKit crate, and the ratchet agrees.
- A session's terminal is still drivable over LiveKit after a remote reach. This is node 1's
  `a_session_connected_locally_becomes_drivable_over_livekit_without_another_call` and the existing
  terminal-bridge suites.
- `ensure_exposed_terminal` exposes once under concurrency and re-exposes after a finished exposure,
  proven with an in-memory exposure.

## Out of Scope
- Shortening the lock held across the join (a session's join blocks other sessions' joins). It is
  recorded as a TODO at wrap if still true.
