# Code index warms on demand, with live LSP progress — PRD

**Date**: 2026-10-18
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Session code pane](../session-code-pane.md) — the pane warms its worktree's index when
  opened, talks to the host over one bidirectional stream, shows rust-analyzer's own progress live, holds
  navigation until the index can answer, underlines navigable symbols on hover, and shows hover in a bounded card.
- **Related Feature**: [Session drawer](../session-drawer.md) — the header's indexing indicator follows the
  worktree's real index state, including loads started by the agent's code tools or after a host restart.
- **Related Feature**: [Code index service](../../../../packages/tddy-index-daemon/docs/code-index-service.md)
  (technical) — per-worktree progress is folded from the language server's first notification, can be watched
  without starting a load, and navigation answers say whether the index was loaded.

## Summary

Today the code index starts loading only when certain kinds of session start, and its progress is remembered per
session in memory. A Code pane opened on any other session — after a host restart, a sandboxed, tool or split
start — reads *Code index not warmed for this session*. Hovering a symbol then answers "No definition found"
while rust-analyzer is still loading, which reads as a broken index.

After this change:

1. **Opening the Code pane warms its worktree's index**, and so does the session agent's first code-tool call
   (`Lsp*`, `restructure_*`).
2. **Progress is rust-analyzer's own**, folded from `$/progress` and `experimental/serverStatus` from the moment
   the server starts, so anyone who looks — at any time — sees every phase running now, its message and its
   percentage, updated in real time.
3. **The Code pane talks to the host over one bidirectional `CodeSession` stream**: progress is pushed, and
   hover, definition, references and a definition probe travel on the same stream.
4. **No premature answers**: a request made while the index loads is held and answered when it is ready; the
   card says the index is loading, with live progress. "No definition found" appears only once the index has
   loaded.
5. **Navigable symbols are underlined on plain hover**, with a pointer cursor, and a plain click goes to the
   definition.
6. **Hover opens a bounded floating card** anchored to the symbol, with rendered markdown, a scrolling body and
   its actions always visible.

## Background

- Reported 2026-10-18: hovering in the Code pane did nothing visible; the strip stayed on *not warmed* and the
  card said "No definition found" (screenshot 08.29).
- The hover answer filled the whole preview height for a long doc comment and its markdown headings ran into the
  following paragraph (screenshot 08.35).
- Two deferred items from #652 describe the root causes:
  [2026-10-18-code-index-warm-is-followed-by-session-record.md](../../../dev/todo/2026-10-18-code-index-warm-is-followed-by-session-record.md)
  and
  [2026-10-18-navigation-answers-empty-while-the-index-loads.md](../../../dev/todo/2026-10-18-navigation-answers-empty-while-the-index-loads.md).
- The index daemon already reads rust-analyzer's progress, but only while a `Warm` stream is open, and a second
  watcher joining mid-load starts with no phase titles (reads "working").

## Proposed Changes

### What's Changing

**Index daemon (per worktree root)**

- The root's graph-load watcher (`GraphLoad`, attached on the first use of a server) keeps the whole fold, not
  only the latch: the set of **active phases** (title, message, percentage per progress token), the furthest
  point reached, and the state — `idle` (no server), `loading`, `ready`, `failed`. Watchers read the latest
  value, so joining mid-load shows the phases already running.
- New `code_index.WatchRoot(workspace_root)` stream: follows that state **without** starting a server. A root
  with no server reports `idle`, and starts reporting the moment anything — the Code pane, a navigation request,
  an agent's code tool — spawns one.
- `Warm` becomes "start the server, then `WatchRoot` until ready" — same contract, now carrying active phases.
- `Definition`, `References`, `Hover` answers carry the **index state** at the time of the answer
  (`loading` / `ready`), so an empty list during loading is distinguishable from "does not resolve".

**Daemon (`code_navigation`)**

- New bidirectional `CodeSession(stream CodeSessionRequest) returns (stream CodeSessionEvent)`:
  - first frame `Open { session_token, project_id, worktree_path }`, authorised like navigation
    (`resolve_listed_worktree`); opening it warms the worktree's index;
  - pushes `IndexProgress` (state, active phases, furthest) for as long as the stream is open — loading, ready,
    and any later reload;
  - accepts `Hover`, `Definition`, `References`, `Probe` and `Cancel`, each with a client `request_id`; replies
    carry the same id;
  - a `Hover` / `Definition` / `References` asked while the index is loading is **held** and answered once the
    index is ready; the event stream says so (`Held { request_id }`) so the pane can show a loading card;
  - a `Probe` (is there a definition here?) is **not** held: while loading it answers `unknown`.
- `WatchCodeIndex(session)` (the header) resolves the session's worktree and follows `WatchRoot` — passive, so
  the header shows loads started by anyone and never starts one itself. The in-memory per-session record
  (`SessionIndexProgress`) is removed; the session-start warm stays as a trigger.
- The unary `Definition` / `References` / `Hover` stay for other callers and pass the index state through.

**Agent code tools (MCP)**

- An agent's `Lsp*` and `restructure_*` calls that reach the index start its server as today; because progress
  is now per root and watchable, the header and any open Code pane show that load live.
- `Lsp*` results answered while the index is loading say so (e.g. *index still loading — Indexing 38%*) instead
  of returning an empty result the agent would read as "nothing found".

**Web (Code pane)**

- The pane opens one `CodeSession` while mounted (keyed by project + worktree) and closes it on unmount.
- The status strip reads progress from that stream: `Indexing — <phase> <n>%` plus the phase's message; when
  more than one phase runs, `+N more`, with every active phase in the strip's tooltip. `ready` reads *Code index
  ready*; `idle` cannot occur while the pane is open. *Code index not warmed for this session* is removed.
- **Hover**: after the existing dwell the pane sends a `Probe` and a `Hover`.
  - `Probe` says definable → the identifier is **underlined** with a pointer cursor; a **plain click** goes to the
    definition (ctrl/cmd-click keeps working). The answer is cached per file and position.
  - The hover card opens **anchored to the identifier** (below it, flipping above when there is no room), at most
    `min(40rem, pane width − 2rem)` wide and `20rem` tall; the body scrolls, the *References* / *Close* actions
    stay visible, and the markdown is rendered (headings, paragraphs, code).
  - While the index loads the card shows *Index loading — <phase> <n>%* with the live progress, then fills in
    when the held answer arrives. Moving to another identifier or closing the card cancels it.
- **Go to definition** while loading shows the same held state in the notice; "No definition found" only when
  the answer came from a loaded index.

### What's Staying the Same

- Navigation is offered for `.rs` files only; one rust-analyzer per worktree root; the index daemon's idle
  eviction (`A_SERVER_MAY_SIT_UNUSED_FOR`).
- The session-start warm for workspace and non-sandboxed claude/cursor starts.
- The header indicator's appearance: shown while loading or failed, hidden otherwise.
- Restructure plans (`OpenPlan` / `WatchPlan` / `RunPlan`) and their dialog.

## Impact Analysis

### Technical Impact

- **Protos**: `code_index.proto` (`WatchRoot`, `IndexProgress` state + active phases, index state on three
  answers) and `code_navigation.proto` (`CodeSession` and its frames, `CodeIndexProgress` mirrors the new
  fields). Generated TS in `tddy-web/src/gen` and `tddy-rust-typescript-tests/gen`.
- **tddy-index-daemon**: `graph.rs` (per-root fold and watch), `warm.rs` (forwards the watch — removes
  `narrate_until_loaded`'s nesting), `navigation.rs` (index state), `service.rs` (`WatchRoot`). `index.rs` is
  already over budget — no growth there beyond the call that hands out the watch.
- **tddy-code-restructuring**: `ServerChatter` exposes the active phases (it already tracks per-token titles).
- **tddy-daemon-rpc**: `CodeSession` handler (new module), `WatchCodeIndex` re-sourced, `code_index_warmup.rs`
  shrinks to a trigger. **tddy-daemon** wiring: net ≤ 0 lines in `runtime.rs`.
- **tddy-lsp-executor**: `Lsp*` loading message.
- **tddy-web**: `WorktreeCodePane`, `CodeBlock` (underline, plain click), `CodeNavigationOverlays` (floating
  card), `useCodeNavigation` (stream-backed), `CodeIndexStatusStrip`, a new `useCodeSession` hook.
  `SessionMainPane.tsx` (over budget) is not touched.
- **Transport**: bidi runs on both wires that reach a host — LiveKit and the desktop IPC bridge — as
  `AcpService.Session` and the terminal already do.

### User Impact

- Opening Code always shows a moving progress line within moments, never *not warmed*.
- No "No definition found" while the index is loading; requests complete on their own once it is ready.
- Navigable symbols are discoverable by hover; a plain click navigates.
- Long hover docs no longer cover the preview.

## Implementation Plan

1. Per-root fold and `WatchRoot` in the index daemon; `Warm` on top of it; index state on navigation answers.
2. `CodeSession` and the re-sourced `WatchCodeIndex` in the daemon; remove `SessionIndexProgress`.
3. `Lsp*` loading message.
4. Web: `useCodeSession`, strip, held requests, probe + underline + plain click, floating card.
5. Tests per layer — index daemon acceptance over `fake_lsp`, daemon acceptance over the stand-in index daemon,
   Cypress component specs over `anInMemoryRpcBackend` with a bidi `CodeSession` fake.

## Acceptance Criteria

- [ ] Opening the Code pane on a worktree with no running index starts it, and the strip shows phase, percentage
      and message changing in real time until *Code index ready* ([Session code pane](../session-code-pane.md))
- [ ] A Code pane opened while a load is already running shows the phases already in progress, not "working"
- [ ] Two phases running at once both appear (`+1 more`, full list in the tooltip)
- [ ] The agent's first `Lsp*` / `restructure_*` call on a cold worktree makes the header indicator show live
      progress ([Session drawer](../session-drawer.md))
- [ ] After a host restart, opening Code on an existing session warms and shows progress
- [ ] The header indicator never starts a load
- [ ] Hover / definition / references asked during loading are held, the card or notice shows *Index loading*
      with live progress, and the answer appears by itself when ready
- [ ] "No definition found" is shown only for an answer from a loaded index
- [ ] An `Lsp*` call during loading tells the agent the index is loading, not an empty result
- [ ] Hovering an identifier with a definition underlines it and shows a pointer cursor; one without stays plain;
      nothing is underlined while the index loads
- [ ] A plain click on an underlined identifier opens its definition
- [ ] The hover card is anchored to the identifier, bounded to `min(40rem, pane − 2rem)` × `20rem`, scrolls its
      body, keeps *References* / *Close* visible and renders markdown headings as headings
- [ ] Closing the pane closes the stream; moving off an identifier cancels its held hover

## References

### Affected Features (Complete List)
- [Session code pane](../session-code-pane.md) — warm on open, CodeSession, progress, held answers, underline,
  hover card
- [Session drawer](../session-drawer.md) — header indicator follows the worktree's index state

### Related Documentation
- `packages/tddy-index-daemon/docs/code-index-service.md`, `packages/tddy-code-restructuring/docs/readiness-and-gates.md`
- `packages/tddy-web/docs/session-start-and-indexing-progress.md`, `packages/tddy-web/docs/code-navigation.md`
- `packages/tddy-daemon/docs/code-navigation-service.md`
- Changeset: [2026-10-18-code-index-warm-on-demand.md](../../../dev/1-WIP/2026-10-18-code-index-warm-on-demand.md)
