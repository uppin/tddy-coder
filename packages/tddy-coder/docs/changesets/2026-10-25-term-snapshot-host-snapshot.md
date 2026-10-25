# 2026-10-25 — The session participant serves `StreamSessionTerminalIO`
**Type:** Fix

`#term-snapshot` 1/4 ([#760](https://github.com/uppin/tddy-coder/pull/760)). The coder's terminal
coordinate answers eight of nine methods; only `WatchTerminalControl` is refused. A bidi `SNAPSHOT`
open of a coder-served terminal gets the same snapshot as the daemon's, pinned by
`two_server_parity_acceptance.rs`'s
`both_wirings_open_one_terminal_with_the_same_snapshot_over_the_bidi_stream` (which replaces the
test that pinned the refusal).
