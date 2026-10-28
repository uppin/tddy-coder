# 2026-10-28 — Context reader resolves a path, then opens it: a symlink swapped in between is followed

**Category:** Future enhancement (pre-existing; surfaced by `context-sync-symlinks` #784 review)
**Source:** `docs/dev/1-WIP/2026-10-28-context-sync-symlinks.md` § Validation Results

Both the manifest walk (`packages/tddy-sandbox/src/context_manifest.rs`) and the reader
(`packages/tddy-session-files/src/context_files.rs`) `canonicalize` a path, check it against the
allow-list, and only then open it by path. On the co-located path an agent that can write the
worktree could replace a checked file with a symlink in that window, and the open would follow it.
The split path's codebase host is not writable by the agent, so it is exposed only when co-located.

## What would close it

Open with `O_NOFOLLOW` relative to a directory fd obtained during the walk (`openat`), or re-check
the opened file's `fstat` identity against the checked path, so the bytes served are the bytes the
allow-list approved.
