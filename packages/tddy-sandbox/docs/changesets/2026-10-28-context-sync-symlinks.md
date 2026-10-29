# 2026-10-28 — Context manifests carry symlinks as links

**Type:** Fix

`ContextEntry.symlink_target`; the walk lists an allow-listed link (glob-named or a `dir/**` root)
as a link and pulls its target in one hop, gated on `git check-ignore` (fail-closed),
`CONTEXT_SECRET_GLOBS` at every depth and `is_withheld_through_a_link`; preamble links are followed.
`context_reach` is the walk's answer the reader shares; `prepare_context_write` guards every write.
Split into `context_manifest/{links,walk,git_ignore}`. PR [#784](https://github.com/uppin/tddy-coder/pull/784);
cross-package entry: [docs/dev/changesets/2026-10-28-context-sync-symlinks.md](../../../../docs/dev/changesets/2026-10-28-context-sync-symlinks.md).
