# 2026-10-18 — Code pane shows the code-index load as it happens
**Type:** Feature

A new session's index warm took 7m31s on this workspace and the only sign of it, the session header,
read *Indexing — Roots Scanned 100%* throughout. Two parts:

- **`tddy-code-restructuring` / `tddy-index-daemon`** — `Warm`'s `phase`/`percentage` are the current
  phase's (`ServerChatter::current()`), not the furthest of any phase; rust-analyzer's early
  *Roots Scanned* at 100% no longer hides the minutes that follow. See
  [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md).
- **`tddy-web`** — `CodeIndexStatusStrip` at the bottom of the Code pane, fed by
  `useCodeIndexProgress`, which the header indicator shares. See
  [session-start-and-indexing-progress.md](../../../packages/tddy-web/docs/session-start-and-indexing-progress.md#indexing-indicator)
  and [session-code-pane.md](../../ft/web/session-code-pane.md#indexing-indicator).

Deferred to the backlog: following the warm by worktree so a session the host no longer remembers is
still followed (`2026-10-18-code-index-warm-is-followed-by-session-record`), and navigation saying the
index is loading instead of answering empty (`2026-10-18-navigation-answers-empty-while-the-index-loads`).

PR: [#652](https://github.com/uppin/tddy-coder/pull/652).
