# Session start and indexing progress in the session UI - PRD

**Date**: 2026-10-03
**PRD Type**: Enhancement

## Affected Features

- **Primary**: [Session drawer](../session-drawer.md) and the create-session pane — progress while a session's worktree is prepared and indexed.
- **Related**: [Warm code-intelligence daemon](../../coder/warm-code-intelligence-daemon.md) — warm on session worktree start.

## Summary

Creating a session shows a disabled button until the call returns, though behind it a worktree is
fetched and created and, optionally, semantically indexed. After start, nothing says the code index
is still loading, so the first navigation stalls for minutes unexplained. This PRD reports start
**phases** while the session starts and the **code index's warm-up** after it: the session UI shows
"creating worktree", "indexing (semantic)", then a code-index indicator with phase and percentage
until the index is ready.

## Proposed Changes

1. `StreamStartSession` gains a `StartPhase` event (`worktree`, `semantic_index`, `agent`), emitted as
   each step begins and ends; the create pane shows the current phase.
2. When a session's worktree is ready and the daemon has `index_daemon:`, the daemon starts `Warm` for
   it in the background (the index daemon itself stays lazily spawned by this request) and keeps the
   latest `IndexProgress` per session.
3. A session-scoped `WatchCodeIndex(session)` stream (on the `code_navigation` service) delivers that
   progress; the session header shows "Indexing — <phase> <n>%" until `ready`, then nothing.
4. Warm failures show as an indicator error with the reason, never block the session.

## Acceptance Criteria
- [ ] Starting a session streams `worktree`, then `agent` phases (plus `semantic_index` when enabled) before the result.
- [ ] The create pane shows the current phase text while submitting.
- [ ] A session on a Rust worktree triggers `Warm` once its worktree exists; `WatchCodeIndex` delivers phase/percentage and a final ready.
- [ ] The session header shows the indexing indicator until ready, then hides it.
- [ ] Without `index_daemon:` no warm starts and no indicator shows.
- [ ] A warm failure shows its reason in the indicator; the session stays usable.
