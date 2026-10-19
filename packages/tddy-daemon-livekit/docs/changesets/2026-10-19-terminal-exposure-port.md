# 2026-10-19 — LiveKit implements the terminal-exposure port

**Type:** Architecture

`#satellite` 6/13 ([#628](https://github.com/uppin/tddy-coder/pull/628)); cross-package entry:
[2026-10-19-satellite-6-terminal-exposure-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-6-terminal-exposure-port.md).

`LiveKitTerminalExposure` (`from_config`, `expose`) holds the token mint, the join and the metadata republish
that lived in `tddy-cli-sessions`; the republish is `spawn_session_metadata_republisher`.
