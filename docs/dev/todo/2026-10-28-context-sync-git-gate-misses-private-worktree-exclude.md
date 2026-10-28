# 2026-10-28 — Context sync's git gate ignores a linked worktree's private exclude file

**Category:** Deferred from `context-sync-symlinks` (#784)
**Source:** PR [#784](https://github.com/uppin/tddy-coder/pull/784) review (Decisions (gitignore gate)); history: [2026-10-28-context-sync-symlinks.md](../changesets/2026-10-28-context-sync-symlinks.md)

A file pulled into an agent's context only because an allow-listed symlink reaches it travels only
if git does not ignore it. The walk asks `git check-ignore --stdin -z`
(`packages/tddy-sandbox/src/context_manifest.rs`), which does not read a **linked worktree's private**
`<gitdir>/info/exclude` — and every daemon-created session checkout is a linked worktree.
`worktree_files` feeds that file in explicitly; this gate does not.

The daemon's own exclude writers use the common dir's `info/exclude`, which *is* honoured, so the gap
is a rule someone placed only in the private file: that path would travel through a link.

## What would close it

Pass the private exclude file to the query (`-c core.excludesFile=` is the wrong knob — it replaces
the user's global file; feed it the way `worktree_files` does), and pin it with a test whose ignore
rule lives only in a linked worktree's private exclude.
