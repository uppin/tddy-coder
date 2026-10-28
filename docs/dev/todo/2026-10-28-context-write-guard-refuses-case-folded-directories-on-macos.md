# 2026-10-28 — Context writes refuse an honest manifest whose directories differ only in case, on macOS

**Category:** Deferred from `context-sync-symlinks` (#784)
**Source:** `docs/dev/1-WIP/2026-10-28-context-sync-symlinks.md` § Validation Results (final pass)

`prepare_context_write` (`packages/tddy-sandbox/src/context_manifest/links.rs`) requires
`canonicalize(parent) == canonicalize(context_dir).join(logical parent)`. On APFS `canonicalize`
returns the case stored on disk, so a Linux codebase host serving both `.claude/Skills/a.md` and
`.claude/skills/b.md` to an agent on a Mac fails the second write `PERMISSION_DENIED` ("resolves
outside the context dir"): a tick is marked stale, and at setup the session does not start. Before
#784 the two directories were silently merged.

## What would close it

Compare each component's identity (`dev`/`ino` from `symlink_metadata` while walking) instead of
path strings — or detect the collision and refuse it with a message naming the two spellings.
