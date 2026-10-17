# Changeset: code-pane-index-status-strip

PRD (target): [`docs/ft/web/session-code-pane.md`](../../ft/web/session-code-pane.md) § *Indexing indicator*
(amended 2026-10-18 — the Code pane's status strip; `<phase>` is the current phase)

## Problem

A new session's index warm took 7m31s on this workspace, and the only sign of it — the session header —
read *Indexing — Roots Scanned 100%* the whole time. Two causes:

1. `tddy-index-daemon`'s warm stream filled `IndexProgress.phase`/`percentage` from
   `ServerChatter::furthest()` — the highest percentage of **any** phase. rust-analyzer finishes
   *Roots Scanned* at 100% within seconds, so every later phase was hidden behind it. The proto already
   documented `phase`/`percentage` as the phase's own progress and `furthest` as the cross-phase figure;
   the code did not match.
2. The Code pane, where a definition or hover that finds nothing is read, showed no index state at all.

## State B

- `ServerChatter` (`tddy-code-restructuring`) tracks the **current** phase: the phase of the token that
  reported most recently, with the highest percentage *that token* reached (`current()`, beside the
  unchanged `furthest()`). `warm.rs`'s `phase_of` reads `current()`. Test:
  `names_the_phase_running_now_rather_than_the_one_that_reached_the_highest_percentage`.
- `tddy-web`: `useCodeIndexProgress(client, token, sessionId)` follows `WatchCodeIndex` and yields
  `waiting | progress | ready | failed | untracked` (a stream that ends with no message).
  `SessionIndexingIndicator` uses it, unchanged in behaviour. `CodeIndexStatusStrip`
  (`worktree-code-index-status`, `data-state`) renders all five states at the bottom of
  `WorktreeCodePane`, which takes a new optional `sessionId` (passed by `SessionMainPane`). Spec:
  `cypress/component/CodeIndexStatusStrip.cy.tsx` (3).

## Not done

- A session whose warm the host no longer remembers (host restarted) reads *not warmed*; nothing
  re-warms it until the first navigation request. Following the warm by worktree (`code_index.Warm` is
  idempotent) instead of by session record would close that.
- Navigation requests still answer empty while the graph loads, rather than saying it is loading.

## Package docs to update at wrap

- `packages/tddy-web/docs/session-start-and-indexing-progress.md` § *Indexing indicator* — the shared
  `useCodeIndexProgress` hook and the Code pane's `CodeIndexStatusStrip` with its five states; testing table
  gains `CodeIndexStatusStrip.cy.tsx` (3).
- `packages/tddy-web/docs/code-navigation.md` § *Where the code lives* — `CodeIndexStatusStrip.tsx` and
  `useCodeIndexProgress.ts`; `WorktreeCodePane` takes `sessionId`.
- `packages/tddy-index-daemon/docs/code-index-service.md` — `Warm`'s `phase`/`percentage` are the
  current phase's, `furthest` the cross-phase figure.
