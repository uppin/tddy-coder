# 2026-10-28 — One submodule path makes context sync's git gate drop every link pull-in in that stage

**Category:** Deferred from `context-sync-symlinks` (#784)
**Source:** PR [#784](https://github.com/uppin/tddy-coder/pull/784) review (Validation Results (final pass)); history: [2026-10-28-context-sync-symlinks.md](../changesets/2026-10-28-context-sync-symlinks.md)

The walk asks `git check-ignore` once per stage for every path a link would pull in
(`packages/tddy-sandbox/src/context_manifest/{walk,git_ignore}.rs`). A path inside a git submodule
makes git exit 128 (`fatal: Pathspec 'vendor/f.md' is in submodule 'vendor'`), the stage fails closed,
and **every** link's pulled-in files are dropped — not just the submodule's. The only trace is a
`warn`. `.claude/skills -> ../vendor/shared/skills` with skills in a submodule is a common layout.

## What would close it

Decide the policy — submodule paths are never pulled in, or they are asked of the submodule's own
repository — and isolate a failure to the link that caused it, with a test whose link target lives in
a submodule beside one that does not.
