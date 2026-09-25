# 2026-09-23 — `MpscResultStream::into_receiver`

**Type:** Refactor

`#carve` 14/15, PR [#524](https://github.com/uppin/tddy-coder/pull/524), DRY #10 (`809baa5c`).
Cross-package entry: [2026-09-23-carve-lifecycle-destructure.md](../../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md).

`MpscResultStream` gains `into_receiver`, which hands back the channel behind the stream for a
caller that relays its items rather than polling them. `tddy-session-lifecycle` needed it, and with
it deleted its own copy of the type: it re-exports this one at `connection_service::MpscResultStream`.
See [worktree-service.md](../worktree-service.md).

The DRY-target baseline for this crate: its 12 `remote_git_livekit_acceptance` failures
(`tddy-remote-git-repo is not built`) are environmental, and
`a_cached_size_is_served_after_reload_without_recomputing` is a pre-existing race, filed in
`docs/dev/todo/2026-09-24-worktree-size-reload-test-races-the-persist-it-reads.md`.
