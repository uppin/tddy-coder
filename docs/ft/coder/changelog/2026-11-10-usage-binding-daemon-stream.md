# 2026-11-10 — The daemon accounts every session's usage

`#usage-binding` 2/5 ([#891](https://github.com/uppin/tddy-coder/pull/891)).

- **Every launch records where its agent keeps its files** (`usage-context.json`), and the daemon
  evaluates the agent's usage binding against it while the Inspector watches.
- **Removed:** `tddy-coder`'s per-session usage watcher and the `TddyRemote` `TokenUsageUpdated` event.

See [Session token accounting](../session-token-accounting.md).
