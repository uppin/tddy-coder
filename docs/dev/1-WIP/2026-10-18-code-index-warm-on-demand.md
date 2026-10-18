# Changeset: Code index warms on demand, with live LSP progress

**Date**: 2026-10-18
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

[2026-10-18-code-index-warm-on-demand-initial-discovery.md](./2026-10-18-code-index-warm-on-demand-initial-discovery.md)
— two passes: the warm / progress / MCP path (Explore agent), then transports, hover rendering and the code-issue
scan.

## Prerequisites

### ✅ RESOLVED HERE — The code-index warm is followed by session record — [`2026-10-18-code-index-warm-is-followed-by-session-record.md`](../todo/2026-10-18-code-index-warm-is-followed-by-session-record.md)

Planned here: progress becomes per worktree root (`code_index.WatchRoot`), `SessionIndexProgress` is removed,
the Code pane's `CodeSession` warms its worktree on open and `WatchCodeIndex` follows the root passively. Move to
resolved only once `/green` lands it.

### ✅ RESOLVED HERE — Navigation answers empty while the index loads — [`2026-10-18-navigation-answers-empty-while-the-index-loads.md`](../todo/2026-10-18-navigation-answers-empty-while-the-index-loads.md)

Planned here exactly as the entry proposes — an index state on the three answers, through both protos — plus the
`CodeSession` holding a request until ready (the entry rejected *waiting in a unary call*; holding on a stream the
pane can cancel is not that).

### ℹ ANSWERED (partly) — Start phases and code-index warm skip sandboxed, tool and split starts — [`2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md`](../todo/2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md)

The **warm** half no longer matters for the Code pane or the header: any session's index warms when its Code
pane opens or its agent calls a code tool, and the header follows the root. The **start-phase** half is
untouched. Entry stays open; `/wrap-context-docs` narrows it to start phases.

### Code issues

| Item | Verdict | What this change does |
|---|---|---|
| `packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md` (522/500, Restructure required, unclaimed) | ⚠ During | The per-root fold lives in `graph.rs`; `index.rs` grows by at most the accessor that hands out a root's watch. Measured at wrap |
| `packages/tddy-index-daemon/docs/code-issues/complexity-warm-narrate-until-loaded.md` (nesting 5) | ✅ Resolved here | `Warm` forwards the root's progress (`forward_until_loaded`, one `select!` in a loop); `narrate_until_loaded` is deleted. Issue file left for wrap to delete |
| `packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md` (674/500) | ⚠ During | Not touched — the pane already has `projectId` / `worktreePath` |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` (816/500) | ⚠ During | Not touched — the `Lsp*` loading message lives in `tddy-lsp-executor` |
| `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` (1,777) | ⚠ During | Wiring net ≤ 0 (`with_index_progress` goes away) |
| `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` | — Unrelated | mechanisms 2–3 are CLI concerns |
| `tddy-lsp`, `tddy-lsp-executor` | not analyzed | no `docs/code-issues/` |

No claimed issue lies in this change's path.

## Affected Packages

- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) · [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md) — per-root progress fold and `WatchRoot`; `Warm` on top of it; index state on navigation answers
- **tddy-code-restructuring**: [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md) — `ServerChatter::active_phases`
- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `code_navigation.proto`: `CodeSession`, progress fields, index state
- **tddy-daemon-rpc**: [architecture.md](../../../packages/tddy-daemon-rpc/docs/architecture.md) — `CodeSession` handler; `WatchCodeIndex` follows the root; `SessionIndexProgress` removed
- **tddy-daemon**: [code-navigation-service.md](../../../packages/tddy-daemon/docs/code-navigation-service.md) — wiring
- **tddy-lsp-executor**: [README.md](../../../packages/tddy-lsp-executor/README.md) — `Lsp*` answers say the index is loading
- **tddy-web**: [code-navigation.md](../../../packages/tddy-web/docs/code-navigation.md) · [session-start-and-indexing-progress.md](../../../packages/tddy-web/docs/session-start-and-indexing-progress.md) — `useCodeSession`, strip, held requests, underline, floating hover card

## Related Feature Documentation

- [PRD-2026-10-18-code-index-warm-on-demand.md](../../ft/web/1-WIP/PRD-2026-10-18-code-index-warm-on-demand.md)
- [Session code pane](../../ft/web/session-code-pane.md), [Session drawer](../../ft/web/session-drawer.md)

## Summary

The code index's progress becomes a per-worktree-root fold of rust-analyzer's own notifications that anyone can
watch without starting a load. The Code pane opens a bidirectional `CodeSession` that warms its worktree, receives
that progress live, and carries hover / definition / references / probe requests, which are held while the index
loads. Symbols with a definition are underlined on hover, and hover renders in a bounded, anchored card.

## Background

See the PRD. In short: warming happens only at some session starts; progress is per session and in memory; the
Code pane watches but never warms; navigation answers empty while loading; the hover card grows without bound.

## Scope

- [x] **Index daemon**: per-root fold + `WatchRoot`; `Warm` on it; index state on `Definition` / `References` / `Hover`
- [x] **Chatter**: `ServerChatter::active_phases`
- [ ] **Protos**: `code_index.proto`, `code_navigation.proto` (+ TS codegen)
- [x] **Daemon**: `CodeSession`; `WatchCodeIndex` re-sourced; `SessionIndexProgress` removed
- [x] **Agent tools**: `Lsp*` loading answer
- [x] **Web**: `useCodeSession`, strip, held requests, probe underline + plain click, floating hover card
- [ ] **Package documentation**
- [~] **Testing**: all acceptance tests passing — tddy-index-daemon acceptance tests written (`root_progress_acceptance.rs`, 6, failing as expected); tddy-web Cypress component specs written (`WorktreeCodePaneCodeSession.cy.tsx` 8, `CodeHoverCard.cy.tsx` 3, `CodeIndexStatusStrip.cy.tsx` 2 rewritten — 13, all failing as expected); tddy-daemon acceptance tests written (`code_session_acceptance.rs` 7, `code_index_warmup_acceptance.rs` +2, all failing as expected); tddy-lsp-executor `lsp_definition_while_the_index_loads_tells_the_agent_it_is_loading` written, failing as expected
- [ ] **Code quality**: clippy / fmt scoped, CI green

## Technical Changes

### State A (Current)

- `GraphLoad` (`tddy-index-daemon/src/graph.rs`) — `watch<bool>` latch, attached in `WorkspaceIndex::record_use`
  when a server is first handed out; folds `ServerChatter::default()` until `quiescent`, then ignores everything.
- `serve_warm` (`warm.rs`) — `client_for` → own `ServerChatter::unthrottled()` over a fresh notification
  subscription (no replay) → one `IndexProgress{line, phase, percentage, furthest, ready}` per distinct line → `ready`
  on the latch. A second warm joining mid-load reads phases as `"working"`.
- `navigation.rs` — answers immediately via `client_for`; empty while loading; no state in the answer.
- `code_navigation.WatchCodeIndex(session)` — follows `SessionIndexProgress` (in-memory, per session), filled only
  by `warm_for_session` from `IndexWarmupObserver` at some session starts; empty stream otherwise.
- MCP `Lsp*` / `restructure_*` → `execute_tool_with_env` → process-global `IndexLspExecutor` → `code_index`;
  spawns the server via `client_for`, reports nothing.
- Web: `CodeIndexStatusStrip` ← `useCodeIndexProgress(WatchCodeIndex)`; navigation via unary RPCs
  (`codeNavigationApi.ts`); `CodeBlock` hover dwell 250 ms, ctrl/meta-click navigates, no underline;
  `HoverCard` is a raw `<pre>` in an `absolute inset-x-3 bottom-3` stack, unbounded.

### State B (Target)

**`code_index.proto`**

```proto
rpc WatchRoot(WatchRootRequest) returns (stream IndexProgress);   // never starts a server

message WatchRootRequest { string workspace_root = 1; }

enum IndexState {
  INDEX_STATE_UNSPECIFIED = 0;
  INDEX_STATE_IDLE        = 1;  // no server holds this root
  INDEX_STATE_LOADING     = 2;  // a server is loading its crate graph
  INDEX_STATE_READY       = 3;  // the graph is loaded and queryable
}

// One work-done progress token in flight, as the language server titled it.
message ActivePhase {
  string          title      = 1;  // e.g. "Indexing", "Roots Scanned", "Fetching"
  string          message    = 2;  // e.g. "120/310 (tddy_core)"; empty when none was given
  optional uint32 percentage = 3;  // absent when the phase never reported one
}

message IndexProgress {            // fields 1–5 unchanged
  IndexState           state  = 6;
  repeated ActivePhase phases = 7; // begin order; empty when idle
}

message DefinitionResponse { repeated CodeLocation locations = 1; IndexState state = 2; }
message ReferencesResponse { repeated CodeLocation locations = 1; IndexState state = 2; }
message HoverResponse      { optional string markdown = 1;       IndexState state = 2; }
```

- **Per-root fold** (`graph.rs`): `GraphLoad` becomes the root's progress publisher —
  `watch<RootProgress>` where `RootProgress { state, phases, furthest, line }` — folded by an **unthrottled**
  `ServerChatter` for the server's whole life (not only until loaded). `state` is `Loading` until `quiescent`, then
  `Ready` for good (a latch, as today). `is_loaded` / `loaded()` / `still_watching()` keep their contracts.
- `WorkspaceIndex` gains `progress_of(root) -> Option<watch::Receiver<RootProgress>>` (the only `index.rs`
  addition) — `None` when no server holds the root.
- `serve_watch_root`: `None` → sends `IDLE`, then polls `progress_of` until a server appears (a cheap wait on the
  index's spawn notification, not a timer loop) and forwards every change; runs until the caller leaves; a server
  that goes away sends `IDLE` again.
- `serve_warm` = `client_for(root)` → forward `progress_of(root)` until `READY` (sent with `ready: true`) or the
  server goes away (`UNAVAILABLE`). `narrate_until_loaded` is deleted.
- **What "loading" means.** A root's `state` (on `WatchRoot`, `Warm` and every navigation answer) is read from the
  server's **latest** `experimental/serverStatus`: `quiescent: true` → `READY`; anything else, including no status
  yet, → `LOADING`; no server → `IDLE`. Not the `GraphLoad` latch: an edit that sends rust-analyzer back to work
  shows as loading again, which is the truth the strip and a held query need. A server that cannot start, or
  exits without being let go, is `FAILED` with `error` until the next attempt. `Warm`'s `ready: true` stays on the
  latch (first quiescence). The status is recorded by `tddy-lsp`'s reader before the response that follows it is
  routed, so a navigation answer's state is deterministic. rust-analyzer always sends the extension because the
  daemon advertises `serverStatusNotification`; a server that never does would read `LOADING` for ever — acceptable
  while rust-analyzer is the only language.

**`ServerChatter::active_phases() -> Vec<ActivePhase>`** (`tddy-code-restructuring`) — one per token between
`begin` and `end`, in begin order: title from `begin`, the latest `message`, the token's highest percentage.

**`code_navigation.proto`**

```proto
rpc CodeSession(stream CodeSessionRequest) returns (stream CodeSessionEvent);

message CodeSessionRequest {
  oneof frame {
    OpenCodeSession open   = 1;   // first frame, exactly once
    CodeQuery       query  = 2;
    CancelCodeQuery cancel = 3;
  }
}
message OpenCodeSession { string session_token = 1; string project_id = 2; string worktree_path = 3; }

enum CodeQueryKind {
  CODE_QUERY_KIND_UNSPECIFIED = 0;
  CODE_QUERY_KIND_HOVER       = 1;
  CODE_QUERY_KIND_DEFINITION  = 2;
  CODE_QUERY_KIND_REFERENCES  = 3;
  CODE_QUERY_KIND_PROBE       = 4;  // "is there a definition here?" — never held
}
message CodeQuery { uint64 request_id = 1; CodeQueryKind kind = 2; string rel_path = 3; SourcePosition position = 4; }
message CancelCodeQuery { uint64 request_id = 1; }

message CodeSessionEvent {
  oneof event {
    CodeIndexProgress progress = 1;  // first event, then every change, for as long as the stream is open
    CodeQueryHeld     held     = 2;  // the query waits for the index to load
    CodeQueryAnswer   answer   = 3;
  }
}
message CodeQueryHeld { uint64 request_id = 1; }

enum Definability {
  DEFINABILITY_UNSPECIFIED = 0;
  DEFINABILITY_DEFINABLE   = 1;
  DEFINABILITY_NONE        = 2;
  DEFINABILITY_UNKNOWN     = 3;  // the index is still loading
}
message ProbeAnswer { Definability definability = 1; repeated CodeLocation locations = 2; }

message CodeQueryAnswer {
  uint64 request_id = 1;
  oneof result {
    HoverResponse      hover      = 2;
    DefinitionResponse definition = 3;
    ReferencesResponse references = 4;
    ProbeAnswer        probe      = 5;
    string             error      = 6;
  }
}

enum CodeIndexState {
  CODE_INDEX_STATE_UNSPECIFIED = 0;
  CODE_INDEX_STATE_IDLE        = 1;
  CODE_INDEX_STATE_LOADING     = 2;
  CODE_INDEX_STATE_READY       = 3;
}
message CodeIndexPhase { string title = 1; string message = 2; optional uint32 percentage = 3; }

message CodeIndexProgress {          // fields 1–6 unchanged
  CodeIndexState          state  = 7;
  repeated CodeIndexPhase phases = 8;
}

message DefinitionResponse { repeated CodeLocation locations = 1; CodeIndexState state = 2; }
message ReferencesResponse { repeated CodeLocation locations = 1; CodeIndexState state = 2; }
message HoverResponse      { optional string markdown = 1;       CodeIndexState state = 2; }
```

- `CodeSession` handler (`tddy-daemon-rpc/src/code_session.rs`, new): authorise `open` with
  `resolve_listed_worktree`; open `code_index.Warm` for the worktree (warms) and, after it, `WatchRoot` (follows
  reloads); forward progress as `progress` events. Each query: forward to `code_index`; an answer with state
  `LOADING` for hover / definition / references → send `held`, wait for `READY` on the watched progress, re-ask,
  send `answer`; a probe never waits (`UNKNOWN` while loading; otherwise `DEFINABLE` + locations or `NONE`).
  `cancel` drops a held query. A query before `open`, or a second `open`, ends the stream `INVALID_ARGUMENT`.
- `WatchCodeIndex(session)`: authorise as today, resolve the session's worktree root (`.session.yaml`), forward
  `code_index.WatchRoot` for it — passive, runs until the client leaves; a non-Rust worktree or no index daemon
  ends with no message, as today.
- `code_index_warmup.rs`: `SessionIndexProgress` and the record are removed; `IndexWarmupObserver` opens `Warm` and
  drains it (trigger only). `CodeNavigationServiceImpl::with_index_progress` removed.
- Unary `Definition` / `References` / `Hover` pass `state` through.

**`tddy-lsp-executor`**: when the index answers `LOADING`, an `Lsp*` result carries
`{"index": "loading", "note": …}` beside the tool's own (empty) key — no progress figure, so a loading answer costs no extra call — and a note that the empty answer is not final, instead of a
bare empty result.

**`tddy-web`**

- `useCodeSession(client, {sessionToken, projectId, worktreePath})` — opens the stream on mount, closes on unmount;
  exposes `progress`, and `ask(kind, relPath, position) → Promise` with `held` surfaced through a callback, plus
  `cancel`.
- `CodeIndexStatusStrip` reads `progress` from the session: `Indexing — <phase> <n>%` + the phase's message;
  `+N more` and a tooltip listing every active phase; `ready` → *Code index ready*. The `untracked` text is removed.
- `useCodeNavigation` asks over the session. A held hover shows the card in a loading state (*Index loading —
  <phase> <n>%*, live); a held definition shows the same in the notice; "No definition found" only for a `READY`
  answer.
- `CodeBlock`: on dwell, `probe` (cached per file + position); `DEFINABLE` → `data-definable="true"`, underline,
  `cursor-pointer`; a plain click navigates using the cached locations. Ctrl/meta-click unchanged.
- `HoverCard`: positioned from the identifier's bounding rect relative to the preview (below; above when it would
  overflow), `max-width: min(40rem, 100% - 2rem)`, `max-height: 20rem`, body `overflow-auto`, actions in a
  non-scrolling footer; body rendered by `renderSimpleMarkdown`.

### Delta

| Package | Files |
|---|---|
| tddy-code-restructuring | `src/backends/rust/chatter.rs` (+`ActivePhase`, `active_phases`) |
| tddy-index-daemon | `proto/code_index.proto`, `src/graph.rs`, `src/warm.rs`, `src/watch_root.rs` (new), `src/navigation.rs`, `src/service.rs`, `src/index.rs` (+`progress_of`); tests |
| tddy-service | `proto/code_navigation.proto` |
| tddy-daemon-rpc | `src/code_session.rs` (new), `src/code_navigation.rs`, `src/code_index_warmup.rs` |
| tddy-daemon | `src/runtime.rs` (drop `with_index_progress`); tests |
| tddy-lsp-executor | `src/index_backed.rs`; tests |
| tddy-web | `src/gen/*`, `components/session/{useCodeSession.ts (new), useCodeNavigation.ts, codeNavigationApi.ts, CodeBlock.tsx, CodeNavigationOverlays.tsx, CodeIndexStatusStrip.tsx, WorktreeCodePane.tsx}`, `useCodeIndexProgress.ts`; specs |
| every `CodeIndexService` fake | add `watch_root` (unimplemented) — `tddy-daemon/tests/*`, `tddy-lsp-executor/tests/*` |

## Implementation Milestones

- [x] `active_phases` with unit tests — 5/5 pass (27/27 in `chatter`)
- [x] Per-root fold, `WatchRoot`, `Warm` rebased on it; `narrate_until_loaded` deleted —
      `root_progress_acceptance.rs` 7/7 pass. As built: `graph.rs` `GraphLoad` publishes
      `watch<RootProgress>` (latest-status `quiescent`, `loaded` latch, active phases, current, furthest, line) folded
      by an unthrottled `ServerChatter` for the server's whole life; `watch_root.rs` (new) holds `RootSignals` (a
      `watch<()>` fired when a root gains a server or its start fails/recovers, plus the per-root start-failure record)
      and computes the root's view (`Idle` / `Held(progress)` / `Failed(reason)`) from the existing `graph_load_of` and
      `warmth_of` — so **`index.rs` grows by +10 lines (one field, one accessor, two call sites), not a `progress_of`
      method**. A server that exits unasked reads `FAILED` because the registry still holds its dead entry (reaping
      and shutdown remove the entry before stopping the task). `GraphLoad::is_loaded` / `loaded()` were deleted (their
      only caller was the narration); the latch is `RootProgress::loaded`
- [~] Index state on navigation answers — from `LspClient::server_status()` read through `ServerChatter`;
      `a_definition_asked_*` 2/2 pass. ⚠ `code_index_service_acceptance.rs`
      `definition_returns_the_callee_location_in_the_worktree`, `references_returns_every_reference`,
      `hover_returns_the_type_text` still FAIL (answer `LOADING`, expect `READY`) — see Technical Debt
- [ ] `code_navigation.proto` contract + TS codegen
- [x] `CodeSession` handler; `WatchCodeIndex` on `WatchRoot`; `SessionIndexProgress` removed — `tddy-daemon-rpc/src/code_session.rs` + `code_session/ask.rs` (new — the loop, and asking one query); `code_navigation.rs` delegates `CodeSession`, follows `WatchRoot` in `WatchCodeIndex`, passes `state` through on the unary answers; `code_index_warmup.rs` drains `Warm` as a trigger; `runtime.rs` net −7 lines
- [x] `Lsp*` loading answer — `tddy-lsp-executor/src/index_backed.rs`, on definition, references and hover
- [x] Web: `useCodeSession`, strip, held UI, probe underline + click, floating card — the 13 new/rewritten Cypress
      specs pass; `WorktreeCodePaneNavigation.cy.tsx` moved onto the `CodeSession` host (3 passing); the header
      indicator reads `state` (falls back to `ready` / `error` only when `state` is `UNSPECIFIED`)
- [ ] Docs updated via wrap

## Testing Plan

**Levels.** Each layer is pinned at its own boundary, against the real code of that layer and a fake of the next:

| Layer | Real | Fake | Why |
|---|---|---|---|
| index daemon | `WorkspaceIndex`, `GraphLoad`, real `tddy-lsp` client | `fake_lsp` binary | the fold, the idle-then-loading transition and held state are behaviour of real notification plumbing |
| daemon | `CodeNavigationServiceImpl` via `ServiceEntry::start_bidi_stream`, real `IndexDaemonRegistry` + stand-in process | `code_index` server the test drives | authorisation, holding and cancelling are this layer's logic; the index's answers are not |
| agent tools | `select_lsp_executor` | `code_index` server | the translation of `LOADING` |
| web | `WorktreeCodePane` + hooks, real Connect client | bidi `CodeSession` in `anInMemoryRpcBackend` | what the user sees |

`fake_lsp` gains one mode, **`--loads-crate-graph-on-signal`**: it begins two overlapping phases (`Fetching`,
`Indexing`) with messages and percentages, then holds until a file the test names appears before reporting
`quiescent` — so "joins mid-load" and "held until ready" are deterministic without sleeps.

**Coverage**: every acceptance criterion in the PRD maps to at least one test below; unit tests cover the fold
edge cases (no `begin` seen, percentage-less reports, `end`, overlapping tokens).

## Acceptance Tests

### tddy-index-daemon — `packages/tddy-index-daemon/tests/root_progress_acceptance.rs` (new)

Written; all six fail against the stubs (`WatchRoot` UNIMPLEMENTED, `state` UNSPECIFIED, Warm carries no phases).

- `watching_a_root_no_server_holds_reports_idle_and_starts_none`
- `watching_a_root_reports_the_load_a_navigation_request_started`
- `watching_a_root_whose_server_cannot_start_reports_it_failed_with_the_reason`
- `a_watch_that_joins_mid_load_names_every_phase_already_running`
- `a_warm_reports_ready_only_once_the_graph_is_loaded_and_carries_the_active_phases`
- `a_definition_asked_while_the_graph_loads_says_loading`
- `a_definition_asked_once_the_graph_is_loaded_says_ready`

### tddy-daemon — `packages/tddy-daemon/tests/code_session_acceptance.rs` (new)

- `opening_a_code_session_warms_its_worktree_and_streams_progress_until_ready`
- `a_hover_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready`
- `a_cancelled_held_hover_is_never_answered`
- `a_probe_while_the_index_loads_answers_unknown_without_waiting`
- `a_probe_on_a_symbol_with_a_definition_answers_definable_with_its_location`
- `a_code_session_on_a_worktree_the_project_does_not_list_is_refused`
- `a_query_before_open_ends_the_session`

### tddy-daemon — `packages/tddy-daemon/tests/code_index_warmup_acceptance.rs` (rewritten)

- `watch_code_index_follows_a_load_another_caller_started_and_never_warms`
- `watch_code_index_for_a_session_after_a_restart_follows_its_worktree` (no session record exists)
- `watch_code_index_reports_an_index_that_failed_with_its_reason`
- kept: `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists`, `without_an_index_daemon_no_warm_starts`, `another_users_watch_of_the_session_is_refused_and_shows_no_progress`
- removed (record no longer exists): `watching_a_session_nothing_warmed_ends_at_once_with_nothing`

### tddy-lsp-executor — `packages/tddy-lsp-executor/tests/lsp_tools_via_index.rs`

- `lsp_definition_while_the_index_loads_tells_the_agent_it_is_loading`

### tddy-web — `packages/tddy-web/cypress/component/WorktreeCodePaneCodeSession.cy.tsx` (new)

- [x] `opening the pane opens a code session for its worktree and shows live progress until ready`
- [x] `shows every phase the index is running at once`
- [x] `a hover while the index loads shows it loading and fills in when answered`
- [x] `go to definition while the index loads never says no definition found`
- [x] `says no definition found only for an answer from a loaded index`
- [x] `underlines an identifier that has a definition and leaves one without plain`
- [x] `a plain click on an underlined identifier opens its definition`
- [x] `closing the pane ends its code session`

### tddy-web — `packages/tddy-web/cypress/component/CodeHoverCard.cy.tsx` (new)

- [x] `the hover card opens beside the hovered identifier`
- [x] `a long hover stays within its bounds and keeps its actions visible`
- [x] `renders hover markdown headings as headings`

### tddy-web — `packages/tddy-web/cypress/component/CodeIndexStatusStrip.cy.tsx` (rewritten onto `CodeSession`)

- [x] kept: `shows the phase running now and its message, then that the index is ready`; `names the reason when the code session fails`. Removed: `untracked`.

**Web test contract** (test ids in `cypress/support/testIds.ts`, page object `worktreeCodePanePage`): `worktree-code-index-status-more` (`+N more`, `title` lists every phase); `worktree-code-hover-loading` (*Index loading — <phase> <n>%*); `worktree-code-hover-body` (scrolling rendered markdown); `worktree-code-hover-close`; `worktree-code-notice` (the navigation notice). The strip headline names the phase reported on **most recently** (`CodeIndexProgress.phase` / `percentage`); its detail is that phase's `message` from `phases`. An identifier the probe found definable carries `data-definable="true"` and computes `text-decoration-line: underline` and `cursor: pointer`. The specs import `src/index.css` so Tailwind utilities apply.

### Unit — `packages/tddy-code-restructuring/src/backends/rust/chatter.rs`

- `active_phases_lists_every_token_between_begin_and_end_in_begin_order`
- `an_active_phase_keeps_its_highest_percentage_and_latest_message`
- `a_report_whose_begin_was_not_seen_is_an_untitled_active_phase`
- `an_ended_token_is_no_longer_active`
- `active_phases_fold_reports_the_throttle_does_not_show`

## Technical Debt & Production Readiness

_(populated during development)_

- Agent code tools inside a sandbox jail: the runner registers no `LspExecutor`, so those calls never reach the
  host index and do not warm it. Out of scope; recorded as a TODO at wrap.
- **`WatchCodeIndex` resolving the worktree** reads `.session.yaml` with `tddy_core::read_session_metadata`
  rather than `peer_session_answer::resolve_worktree_root_in_session_dir`, because that helper turns "no
  record" and "no worktree recorded yet" into the same `FAILED_PRECONDITION`. Here they differ: a session
  whose worktree does not exist yet ends with no message (no indicator, as before), and an unreadable record
  is `FAILED_PRECONDITION`. A worktree without `Cargo.toml`, or no `index_daemon:` section, also ends empty.
- **`CodeSession` decisions made in green** (worth a look at `/validate-changes`):
  - A `Warm` the index refuses is only logged: `WatchRoot` reports the root `FAILED` with the reason, which is
    what the pane shows. The session stays open.
  - A held query is released by a `READY` that arrives *after* it was asked (a progress counter), so a stale
    `READY` never spins a re-ask loop and a `READY` that raced the `LOADING` answer is not missed. A root that
    reports `FAILED` answers every held query with `error: "the code index failed: <reason>"`.
  - `WatchRoot` ending ends the session `UNAVAILABLE`; held queries then get no answer (the stream is over).
  - A query reusing a `request_id` replaces the earlier one; `cancel` drops a query in flight as well as a held
    one. One loop owns the session and reads frames first (`biased`), so a `cancel` sent before the index became
    ready is applied before the held query is released.
  - A probe follows the contract strictly: `LOADING` → `UNKNOWN` with no locations, even if the index already
    returned some.
- **Package docs still describe `SessionIndexProgress` / `with_index_progress`**:
  `tddy-daemon-rpc/docs/architecture.md`, `tddy-daemon/docs/code-navigation-service.md`,
  `tddy-daemon/docs/daemon-endpoint.md`, `tddy-daemon/docs/code-issues/complexity-runtime-build.md` — updated at
  wrap.
- **Local verification preconditions**: `tddy-daemon-rpc`'s jail suites need `tddy-tools` and
  `tddy-sandbox-runner` built (`./test` builds `tddy-tools`; plain `cargo test` does not). Not a regression.
- ⚠ Index daemon: the three navigation tests in `code_index_service_acceptance.rs` that now expect `READY` ask a
  **cold** root with `fake_lsp --loads-crate-graph`, which reports `quiescent: true` only ~280 ms after the
  handshake; the fake answers `definition` at once, so the honest state is `LOADING` (no status yet) — the exact
  case `a_definition_asked_while_the_graph_loads_says_loading` pins. No implementation satisfies both without
  waiting in a unary call (rejected) or racing. Proposed test fix: warm the root to the end before asking, as
  `a_definition_asked_once_the_graph_is_loaded_says_ready` does. Left for the test author.
- Index daemon: `watching_a_root_whose_server_cannot_start_reports_it_failed_with_the_reason` takes ~10 s —
  `tddy-lsp`'s `initialize` against a server that already exited waits out `DEFAULT_REQUEST_TIMEOUT` (10 s) instead
  of failing on EOF. Pre-existing `tddy-lsp` behaviour, not changed here.
- Index daemon: `WatchRoot` and `Warm` copy `ActivePhase.percentage` (`u64` in the chatter) into the proto's `uint32`
  with `as`; LSP percentages are `uinteger` 0–100 by spec.
- Chatter: `ActivePhase`'s doc comment had been inserted between `ServerChatter`'s doc and its struct (so the
  struct lost its doc); moved above `ServerChatter`'s doc.
- Web: `WorktreeCodePaneProps.sessionId` is unused now that the strip follows the pane's `CodeSession` — kept (with a
  `TODO`) because removing it means editing `SessionMainPane.tsx` (over budget) and the Cypress pane driver.
- Web: the hover card's position is computed when it opens and on each re-render; it does not follow the preview
  while it scrolls (the card lives in the preview's non-scrolling wrapper).
- Web: probe answers are cached per file + position and forgotten whenever a file is opened (a re-read file may have
  moved its symbols); an index that goes back to loading after an edit does not clear them.

## Decisions & Trade-offs

- **Bidi `CodeSession`** over warm-RPC + unary navigation — developer's choice (2026-10-18): one stream per pane
  carries progress and queries, which is what makes holding and cancelling cheap. Unary RPCs stay for other
  callers. Both host wires (LiveKit, desktop IPC) are duplex; the browser's own `connect-web` control plane is not,
  and is not how a host client is reached.
- **Hold, don't refuse**, while loading — developer's choice. Probes are the exception: an underline that appears
  minutes later on a token the pointer left is noise.
- **Underline on plain hover**, plain click navigates — developer's choice; costs one probe per dwell, cached.
- **MCP trigger** = the session agent's `Lsp*` / `restructure_*` calls — developer's choice. No new trigger code:
  those already spawn the server; making progress per root is what makes the load visible.
- **Header stays passive** (`WatchRoot`) so only the pane, the agent or a session start warms.
- **Per-root fold in `graph.rs`**, not `index.rs`, because `index.rs` is over budget.
- **Acceptance-test review (2026-10-18), developer approved the recommendations:** (1) `FAILED` joins
  `IndexState` / `CodeIndexState` with `IndexProgress.error`, so a server that cannot start or dies mid-load reaches
  the header and the strip — a server let go for being unused is `IDLE`; (2) `Lsp*` loading answers carry no
  progress figure; (3) the strip names the phase reported on most recently.
- **State from the latest `serverStatus`**, not the latch (see State B) — decided while writing the contract; the
  existing navigation tests' fixture now adds `--loads-crate-graph`, and the three tests warm the root to the end
  (`the_graph_of(..).has_loaded()`) before asking — the planning premise that the fake reports quiescence before it
  answers the first request was wrong (it reports ~280 ms after the handshake), so a cold ask correctly reads
  `LOADING`.

## Refactoring Needed

### From @red (TDD Red Phase)

- `root_progress_acceptance.rs` copies `unary_at`, `a_message_carrying` and the `subject` workspace fixture from
  `code_index_service_acceptance.rs` (1,885 lines) rather than restructuring it; a shared `tests/support/` module
  for both suites would remove the copy.
- `fake_lsp --loads-crate-graph-on-signal` waits `BEFORE_THE_FIRST_PHASE` (100 ms) before its first notification,
  as `--loads-crate-graph` does: the per-root fold must subscribe within that window of the handshake, or it
  misses the `begin`s (which would surface as untitled phases).
- Web: `cypress/support/rpc/codeSessionBackend.ts` (`aCodeIndexHost`) is a stateful fake of the daemon's
  `CodeSession` handler, recording every frame since `tddy-connectrpc-testkit` records unary requests only. If a
  second bidi spec needs frame recording, move it into the testkit (`backend.framesTo(method)`).
- Web: the pane is mounted standalone by `cypress/support/drivers/worktreeCodePaneDriver.tsx` and still passes
  `sessionId`, which the strip no longer needs once it reads the `CodeSession`; drop the prop from the driver if
  green removes it from `WorktreeCodePaneProps`.
- Web: `cypress/support/testIds.ts` has pre-existing duplicate keys (`rpcServiceTree`, `rpcRequestEditor`,
  `rpcInvokeButton`, TS1117), not introduced here.
- Daemon: `packages/tddy-daemon/tests/support/mod.rs` (new) holds the scripted `code_index` fake (one root's
  progress the test moves; `Warm` and `WatchRoot` follow it; navigation answers empty + `LOADING` until `READY`),
  the stand-in index-daemon host and the project + session fixture, for `code_session_acceptance.rs` and
  `code_index_warmup_acceptance.rs`. `code_navigation_acceptance.rs` and `plan_dialog_acceptance.rs` still carry
  their own copies of the stand-in host and a `CodeIndexService` fake; fold them onto `support`.
- Daemon: `code_index_warmup_acceptance.rs` still starts its warm through `warm_for_session(…,
  &SessionIndexProgress::new(), …)` — only in `a_warm_of` and `without_an_index_daemon_no_warm_starts` — so
  removing `SessionIndexProgress` changes exactly those two call sites.
- Daemon: besides `watching_a_session_nothing_warmed_ends_at_once_with_nothing`, two more tests of the old record
  were removed from `code_index_warmup_acceptance.rs`: `watch_code_index_delivers_phase_percentage_and_ready`
  (superseded by `watch_code_index_follows_a_load_another_caller_started_and_never_warms`) and
  `a_warm_failure_is_reported_with_its_reason_and_the_warm_does_not_panic` (the warm becomes a trigger only). The
  failure path is restored through the root instead: `IndexState::FAILED` + `IndexProgress.error`, pinned by
  `watch_code_index_reports_an_index_that_failed_with_its_reason` (daemon) and
  `watching_a_root_whose_server_cannot_start_reports_it_failed_with_the_reason` (index daemon).
- Agent tools: the loading answer carries `index: "loading"` and a note only — the `progress` figure was dropped
  at review (2026-10-18) so the executor never calls `WatchRoot`.

### From /green (web)

- `codeNavigationApi.ts` now holds only `CodePosition`, `CodeLocationRef` and `toLocationRef`; the unary adapter
  (`createCodeNavigationApi`) had no caller left and was removed. A rename to `codeLocations.ts` would fit better.
- `cypress/support/rpc/codeSessionBackend.ts`: `CodeIndexHost.backend(base?)` takes a base backend, so the drawer-level
  `WorktreeCodePaneNavigation.cy.tsx` serves its sessions list and the `CodeSession` from one backend.
- `renderSimpleMarkdown` gained `##`–`######` headings and fenced code blocks (rust-analyzer hovers open with a
  ```` ```rust ```` fence); no unit test pins it beyond `CodeHoverCard.cy.tsx`'s heading case.

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## TODO

- [x] Record initial discovery (`2026-10-18-code-index-warm-on-demand-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-10-18, with the three recommendations)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (scoped locally, full on CI) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-code-index-warm-on-demand-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
