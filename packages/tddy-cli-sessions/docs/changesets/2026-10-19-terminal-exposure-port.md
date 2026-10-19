# 2026-10-19 — The session manager exposes terminals through a port

**Type:** Architecture

`#satellite` 6/13 ([#628](https://github.com/uppin/tddy-coder/pull/628)); cross-package entry:
[2026-10-19-satellite-6-terminal-exposure-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-6-terminal-exposure-port.md).

`ensure_exposed_terminal` replaces `ensure_livekit_terminal`; `pty_terminal_service` is public and LiveKit-free;
`livekit_bridge.rs` and `livekit_terminals.rs` became `pty_terminal_service.rs` and `exposed_terminals.rs`;
`LiveKitTerminalAddress` is gone and `tddy-livekit` left the manifest.
