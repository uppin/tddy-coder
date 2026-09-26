# 2026-09-26 — Holds the session catalog's daemon side

**Type:** Refactor

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry:
[2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md).

`session_reader`, `user_sessions_path`, `session_deletion` and `session_list_enrichment` moved here
from `tddy-session-lifecycle` as one cluster, with `tests/worktree_removal_eligibility.rs`;
lifecycle re-exports all four by name. They are here rather than in `tddy-session-catalog` because
their edges would add 85 packages to `tddy-bsp`'s graph and every caller is daemon-side. New
dependencies `tddy-session-files`, `tddy-projects`, `chrono`, `libc` (unix),
`tddy-daemon-sandbox`, `tddy-daemon-livekit`; dev `tddy-workflow`, `tempfile`. `signal_pid` is
`pub` for lifecycle's `CliSessionManager`. Production lines 1,573 → 2,541. The crate has tests for
the first time: 45 (12 + 27 + 1 inline, 5 in the suite). See [session-catalog.md](../session-catalog.md).
The code issue `complexity-service-stream-acp-replay` was not touched.
