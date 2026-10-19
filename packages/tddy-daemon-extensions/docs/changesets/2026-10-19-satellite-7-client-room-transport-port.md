# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)); cross-package entry:
[2026-10-19-satellite-7-client-room-transport-port.md](../../../../docs/dev/changesets/2026-10-19-satellite-7-client-room-transport-port.md).

The ratchet strikes `tddy-remote-git-repo`, `tddy-session-sync`, `tddy-session-tool-client` and `tddy-terminal-rpc` from `KNOWN_OFFENDERS` and adds `WIRING_WITH_A_LIBRARY` with `a_wiring_crate_that_ships_a_library_names_livekit_only_from_its_binary`.
