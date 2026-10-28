# 2026-10-28 — Context reads and writes check a path, then use it by name: a symlink swapped in between is followed

**Category:** Future enhancement (pre-existing; surfaced by `context-sync-symlinks` #784 review)
**Source:** `docs/dev/1-WIP/2026-10-28-context-sync-symlinks.md` § Validation Results

Both the manifest walk (`packages/tddy-sandbox/src/context_manifest.rs`) and the reader
(`packages/tddy-session-files/src/context_files.rs`) `canonicalize` a path, check it against the
allow-list, and only then open it by path. On the co-located path an agent that can write the
worktree could replace a checked file with a symlink in that window, and the open would follow it.
The split path's codebase host is not writable by the agent, so it is exposed only when co-located.

The **writers** have the same shape in the other direction, and there the agent's side is the
writable one: the split path's context dir is the agent's scratch space. Between
`prepare_context_write` returning and `std::fs::write` (`context_sync/context_writes.rs`), or between
`unlink_if_symlink` and the write, the agent can plant a symlink and the daemon writes through it.
`rewrite_preamble_files` (`tddy-sandbox/src/context_dir.rs`, called by `mark_context_stale`) reads and
rewrites `CLAUDE.md` through a link the agent may have planted. #784's guard closes the escape against
a *peer's* manifest, not against a concurrent agent; none of this is new in #784.

## What would close it

Open with `O_NOFOLLOW` relative to a directory fd obtained during the walk (`openat`), or re-check
the opened file's `fstat` identity against the checked path, so the bytes served are the bytes the
allow-list approved — and write the same way (`openat` + `O_NOFOLLOW` | `O_CREAT`, `O_DIRECTORY`
for each parent).
