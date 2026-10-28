# 2026-10-28 — A tree pulled in through a context-sync link has no aggregate bound

**Category:** Deferred from `context-sync-symlinks` (#784)
**Source:** `docs/dev/1-WIP/2026-10-28-context-sync-symlinks.md` § Open decisions

An allow-listed symlink pulls its target into the agent's context even when no glob names that
target (`walk_context_files`, `packages/tddy-sandbox/src/context_manifest.rs`). A directory target is
listed in full, held only to the per-file `max_attachment_bytes` cap, so a committed
`.claude/x -> node_modules` lists every file of `node_modules` in the manifest.

## Why it is not urgent

The split fetch (`split_context_from_codebase_host`) already refuses a manifest whose **total**
exceeds the host's `max_attachment_bytes`, before a byte is read — so the worst case on the split
path is a session that refuses to start with a message naming the size, not an unbounded transfer.
The co-located path copies locally and has no such total check; the cost there is disk and time.

## What would close it

A bound on what one link may pull in — a file count, a byte total, or both — enforced in the walk so
the manifest, the reader and both sync paths agree, with the refusal naming the link that exceeded
it. Pin it with a manifest test (`.claude/x -> big/` over the bound is left out, with a warning) and
decide whether exceeding it drops the link or fails the sync.
