# 2026-10-21 — `worktree_measurement`

`#satellite` 13/13 (#635). `snapshot_worktree`, `snapshot_worktree_within` and the deadline-bounded
`git_output` / `git_stdout` / `git_stdout_with` moved here from `tddy-daemon-livekit::session_room`,
with the index-lock test, so the session room and `GetWorktreeSnapshot` measure through one function.
