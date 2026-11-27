# 2026-11-27 — Session files and worktrees go through the Fs port

The production disk I/O of `tddy-session-files`, `tddy-session-sync`,
`tddy-subagent-worktree`, `tddy-session-worktree` and `tddy-worktree-service` goes through the
`tddy-fs` port (refactor, no behaviour change): readers, listings, uploads, staging, mirrors and
size walks run behind `<fn>_on` seams with `DiskFs` only at composition roots — service
constructors, request handlers, and the session daemon's wiring. The port itself grows two
additive methods: `Fs::create_new` (exclusive create) and `Fs::copy`, each with `DiskFs` +
`MemFs` implementations and conformance tests.
