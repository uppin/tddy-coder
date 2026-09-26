# 2026-09-26 — Holds the PTY action runtime and per-user settings

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`pty_runtime` and `tddy_user_config` moved here from `tddy-session-lifecycle` as one cluster, with
their 8 inline tests (55 → 63 passed); lifecycle re-exports both by name. New dependency
`tddy-daemon-kernel` (unconditional; no cycle), dev `tempfile`. Production lines 2,348 → 2,526. See
[terminal-session-service.md](../terminal-session-service.md#the-pty-action-runtime-and-per-user-settings).
The code issue `complexity-bridge-open-replay-ack-live` was not touched.
