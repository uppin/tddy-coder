# 2026-11-11 — OpenCode usage from its SQLite store

`#usage-binding 5/5` ([#894](https://github.com/uppin/tddy-coder/pull/894)).

- **OpenCode sessions show usage**: the main session (the worktree's newest root session) and the
  sessions it spawned, nested to any depth, with tokens, cache, reasoning and cost.
- **A generic `sqlite` usage format** lets a spec bind any store with one row per conversation; the
  database is opened read-only, and a busy store keeps the last snapshot.
- Whether OpenCode's output count already includes reasoning is not yet verified against a real store
  ([todo](../../../dev/todo/2026-11-11-opencode-reasoning-overlap-unverified.md)).

See [Session token accounting](../session-token-accounting.md).
