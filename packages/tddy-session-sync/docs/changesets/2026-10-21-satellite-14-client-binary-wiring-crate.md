# 2026-10-21 — The worktree mirror binary is built by a wiring crate

**Type:** Architecture

`#satellite` 14/15 ([#689](https://github.com/uppin/tddy-coder/pull/689)); cross-package entry:
[2026-10-21-satellite-14-client-bins-wiring.md](../../../../docs/dev/changesets/2026-10-21-satellite-14-client-bins-wiring.md).

The `tddy-session-sync` binary is built by `tddy-session-sync-cli`, under the same name. The library
lost its `[[bin]]`, `src/main.rs`, `tests/cli_acceptance.rs` and its `tddy-livekit` and
`env_logger` dependencies; it names no LiveKit crate and builds no binary.
