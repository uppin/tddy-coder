# 2026-10-19 — Terminal exposure through an extension port

**Type:** Architecture

`#satellite` 6/13 ([#628](https://github.com/uppin/tddy-coder/pull/628)). Exposing a session's PTY to participants
outside this host is a port. `tddy-cli-sessions` no longer names `tddy-livekit`.

## What changed

- **Port.** `TerminalExposure` (`expose(at, terminal, metadata_json)`), `TerminalAddress { room, identity }`,
  `ExposedTerminal` (a running exposure that reports when it has ended) and the `NoTerminalExposure` null object,
  in `tddy-daemon-extensions`. [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-terminal-exposure-port-terminal_exposure).
- **Plugin.** `LiveKitTerminalExposure` in `tddy-daemon-livekit` holds what `spawn_livekit_bridge` held: the token
  (`TokenGenerator`, 24 h), the join, and the 30 s metadata republish, now its own function. Credentials leave the
  address; the plugin reads them from config. [LiveKit service](../../packages/tddy-daemon-livekit/docs/livekit-service.md#livekitterminalexposure--serving-a-terminal-in-a-room).
- **Manager.** `CliSessionManager::ensure_livekit_terminal` became `ensure_exposed_terminal(session_id, at, &dyn
  TerminalExposure)`, still single-flight under the write lock; it answers `Ok(false)` for a failed precondition
  (nothing wired). `pty_terminal_service(handle)` is public and built from `tddy-rpc` types only.
  `livekit_bridge.rs` → `pty_terminal_service.rs`, `livekit_terminals.rs` → `exposed_terminals.rs`,
  `LiveKitTerminalAddress` removed. [Module layout](../../packages/tddy-cli-sessions/docs/module-layout.md).
- **Callers.** Lifecycle's `SessionTerminalBridge` impl (`with_terminal_exposure`), Telegram's immediate bridge
  (`with_terminal_exposure`) and `tddy-daemon`'s `build` use the port; the daemon builds the exposure once.
- **Ratchet.** `tddy-cli-sessions` is struck from `KNOWN_OFFENDERS`.

## Not changed

When a terminal is exposed (node 1's `SessionReach` decides), the identity, the room name, the token TTL, and the
republish interval. The lock still spans the join, so a session's join blocks other sessions' joins. The
`pty-relay` client side of `tddy-terminal-rpc` is untouched.

## Verification

Scoped. `tddy-cli-sessions` (`exposed_terminals`, six tests, with an in-memory exposure), `tddy-daemon-extensions`
(unit and the ratchet) and the `session_room_acceptance` suite (29, against a real LiveKit server) pass;
`tddy-telegram-control` passes; `cargo clippy --all-targets -D warnings` and `cargo fmt --check` are clean on the
six touched packages. The full `tddy-session-lifecycle` and `tddy-daemon` suites were not run locally.

## Code issues

Three files over budget grew and their splits are deferred with the developer's consent:
`oversized-file-runtime` 1,818 → 1,832, `oversized-file-telegram-session-control-mod` 539 → 557,
`oversized-file-telegram-session-control-session-start` 713 → 719.
