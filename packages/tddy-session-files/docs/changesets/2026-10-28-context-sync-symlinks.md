# 2026-10-28 — Context reads and syncs understand symlinks

**Type:** Fix

The reader asks `tddy_sandbox::context_reach` for link-reached and read-as-withheld paths; the
syncer recreates, re-points and removes links, refuses entries beneath a served link and links at
preamble paths, and writes only through `prepare_context_write`. `LocalWorktreeSource` answers a
tick's reads from its manifest's walk. One wire mapping (`context_entry_{to,from}_wire`). PR
[#784](https://github.com/uppin/tddy-coder/pull/784); cross-package entry:
[docs/dev/changesets/2026-10-28-context-sync-symlinks.md](../../../../docs/dev/changesets/2026-10-28-context-sync-symlinks.md).
