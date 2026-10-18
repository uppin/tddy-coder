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
- [x] **Protos**: `code_index.proto`, `code_navigation.proto` (+ TS codegen — `tddy-web/src/gen` and `tddy-rust-typescript-tests/gen` identical); `FAILED` + `error` added to both state enums (🆕 vs the State B sketch below)
- [x] **Daemon**: `CodeSession`; `WatchCodeIndex` re-sourced; `SessionIndexProgress` removed
- [x] **Agent tools**: `Lsp*` loading answer
- [x] **Web**: `useCodeSession`, strip, held requests, probe underline + plain click, floating hover card
- [ ] **Package documentation**
- [~] **Testing**: Rust suites green locally, scoped — first /validate-changes run: `root_progress_acceptance` 7/7, `code_index_service_acceptance` 44/44, `tddy-index-daemon --lib` 54/54, `chatter` 27/27, `restructure_tools_via_index` 14/14; after the refactor round (Validation Results → Daemon-side refactor): `code_session_acceptance` 17/17, `code_index_warmup_acceptance` 11/11, `code_navigation_acceptance` 7/7, `plan_dialog_acceptance` 5/5, `lsp_tools_via_index` 15/15. Cypress specs reported green by the web refactor, not re-run at validation. Final scoped gates run separately at pr-wrap step 5; CI not run (no PR)
- [~] **Code quality**: scoped clippy `-D warnings --all-targets` clean (daemon-side refactor); `cargo fmt --check` clean on all seven touched Rust packages (re-checked at final /validate-changes); open clean-code items listed under Validation Results → /validate-changes → Final

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
  INDEX_STATE_FAILED      = 4;  // 🆕 the server could not start, or exited unasked; `error` says why
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
  string               error  = 8; // 🆕 why, when FAILED
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
  CODE_INDEX_STATE_FAILED      = 4;  // 🆕 `error` (field 6, existing) says why
  CODE_INDEX_STATE_NONE        = 5;  // 🆕 pr-wrap: not a Rust worktree, nothing will load; `error` says why
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
  `code_index.WatchRoot` for it — passive, runs until the client leaves; a non-Rust worktree, no `index_daemon:`
  section, or an index daemon not already running (`IndexChannelSource::connect_if_running`) ends with no message.
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
| tddy-web | `src/gen/*`, `components/session/{useCodeSession.ts (new), codeProbes.ts (new, web refactor), useCodeNavigation.ts, codeNavigationApi.ts, CodeBlock.tsx, CodeNavigationOverlays.tsx, CodeIndexStatusStrip.tsx, WorktreeCodePane.tsx}`, `useCodeIndexProgress.ts`; specs |
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
- [x] Index state on navigation answers — from `LspClient::server_status()` read through `ServerChatter`;
      `a_definition_asked_*` 2/2 pass; the three `code_index_service_acceptance.rs` navigation tests now warm
      the root first (`the_graph_of(..).has_loaded()`) and pass (44/44, verified at /validate-changes)
- [x] `code_navigation.proto` contract + TS codegen (`CodeSession`, `CodeIndexState` incl. `FAILED`,
      `CodeIndexPhase`, state on the three answers)
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
- `a_definition_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready`
- `references_asked_while_the_index_loads_are_held_and_answered_once_it_is_ready`
- `a_held_hover_on_an_index_that_then_fails_is_answered_with_the_failure`
- `a_probe_where_nothing_is_defined_answers_none`
- `a_query_reusing_a_held_query_s_id_replaces_it`
- `an_index_let_go_while_the_pane_is_open_is_warmed_again`
- `a_hover_asked_after_the_index_was_let_go_is_held_until_it_is_ready_again`
- `a_code_session_whose_index_stops_following_its_worktree_ends_unavailable`
- `a_code_session_opened_with_a_token_nobody_holds_is_refused`
- `a_code_session_on_a_worktree_that_is_not_a_rust_worktree_has_no_index`
- `an_open_code_session_keeps_the_index_daemon_running_and_lets_it_go_once_closed` (FV1)
- `an_open_code_session_keeps_its_loaded_root_in_use_by_warming_it_again` (FV3)

### tddy-daemon — `packages/tddy-daemon/tests/code_index_warmup_acceptance.rs` (rewritten)

- `watch_code_index_follows_a_load_another_caller_started_and_never_warms`
- `watch_code_index_for_a_session_after_a_restart_follows_its_worktree` (no session record exists)
- `watch_code_index_reports_an_index_that_failed_with_its_reason`
- kept: `a_session_s_rust_worktree_handed_to_the_warm_up_is_warmed_through_the_index_daemon` (renamed from `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists`), `without_an_index_daemon_no_warm_starts`, `another_users_watch_of_the_session_is_refused_and_shows_no_progress`
- `watch_code_index_while_no_index_daemon_runs_ends_with_nothing_and_starts_none`
- `watch_code_index_for_a_session_whose_worktree_is_not_created_yet_ends_with_nothing`
- `watch_code_index_of_a_worktree_without_a_cargo_manifest_ends_with_nothing`
- `watch_code_index_without_an_index_daemon_section_ends_with_nothing`
- `watch_code_index_of_a_session_whose_record_is_unreadable_is_a_failed_precondition`
- removed (record no longer exists): `watching_a_session_nothing_warmed_ends_at_once_with_nothing`

### tddy-lsp-executor — `packages/tddy-lsp-executor/tests/lsp_tools_via_index.rs`

- `lsp_definition_while_the_index_loads_tells_the_agent_it_is_loading`
- `lsp_references_found_while_the_index_loads_are_marked_possibly_incomplete`

### tddy-web — `packages/tddy-web/cypress/component/WorktreeCodePaneCodeSession.cy.tsx` (new)

- [x] `opening the pane opens a code session for its worktree and shows live progress until ready`
- [x] `shows every phase the index is running at once`
- [x] `a hover while the index loads shows it loading and fills in when answered`
- [x] `go to definition while the index loads never says no definition found`
- [x] `says no definition found only for an answer from a loaded index`
- [x] `underlines an identifier that has a definition and leaves one without plain`
- [x] `a plain click on an underlined identifier opens its definition`
- [x] `closing the pane ends its code session`
- [x] added in the web refactor (2026-10-18): `closing a loading hover card cancels its query on the host`;
      `moving to another identifier cancels the held hover of the one before`; `a held go-to-definition reports
      that the index failed to load, and why`; `probes a position once however often the pointer rests on it`;
      `a probe the loading index cannot answer leaves the identifier plain and is asked again once it is ready`;
      `opening another file cancels the probes in flight, and their late answers underline nothing`; `a plain
      click on an identifier without a definition asks nothing` — 15/15 pass

### tddy-web — `packages/tddy-web/cypress/component/WorktreeCodePaneReconnect.cy.tsx` (new, web refactor 2026-10-18)

- [x] `a session dropped under the pane is reopened after a second, saying it is reconnecting meanwhile`
- [x] `waits twice as long after each failed attempt, up to 30 seconds`
- [x] `a reopened session that stayed open for 10 seconds waits a second again after the next drop` (was: *…that
  delivered…*; FV5)
- [x] `a host that accepts each session and drops it at once is backed off, not asked every second` (FV5)
- [x] `a hover held when the session dropped is asked again on the reopened one and fills in`
- [x] `closing the pane while it is reconnecting stops reopening`
- [x] `a session the host refuses is not reopened, and the strip names why`

### tddy-web — `packages/tddy-web/cypress/component/SessionIndexingIndicatorResubscribe.cy.tsx` (new, FV2/FV6)

- [x] `the header shows a load that starts after its first watch ended empty`
- [x] `the header drops what it last said once its watch ends mid-load`
- [x] `the header watches again after 1, 2 and 4 seconds while no index daemon runs`
- [x] `a watch the host refuses is not asked again, and the header names why`

### tddy-web — `packages/tddy-web/cypress/component/CodeHoverCard.cy.tsx` (new)

- [x] `the hover card opens beside the hovered identifier`
- [x] `a long hover stays within its bounds and keeps its actions visible`
- [x] `renders hover markdown headings as headings`
- [x] added in the web refactor: `renders a fenced rust block as one code block, apart from the docs after it`;
      `renders everything after a fence that is never closed as code`

### tddy-web — `packages/tddy-web/cypress/component/CodeIndexStatusStrip.cy.tsx` (rewritten onto `CodeSession`)

- [x] pr-wrap: `says a worktree that is not a Rust worktree has no code index, and why` (`NONE`)
- [x] Web refactor (2026-10-18), strip-only concerns: `names the reason when the index fails to load`; `an index let go while the pane is open reads as starting, never as not loaded`; `says it is reconnecting, and why, while the code session is down`. Removed: `untracked`; `shows the phase running now …` (duplicated the first `WorktreeCodePaneCodeSession` test, W9); `names the reason when the code session fails` (a dropped session now reconnects).

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
- ✅ RESOLVED (test fixed as proposed — see Decisions; 44/44 at /validate-changes) — Index daemon: the three navigation tests in `code_index_service_acceptance.rs` that now expect `READY` ask a
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
- ✅ RESOLVED (web refactor, 2026-10-18) — Web: `WorktreeCodePaneProps.sessionId` removed, with the prop
  `SessionMainPane.tsx` passed (−1 line) and the Cypress pane driver's.
- Web: the hover card's position is computed when it opens and on each re-render of the card; it does not follow
  the preview while it scrolls (the card lives in the preview's non-scrolling wrapper). It now finds its identifier
  by position (`data-identifier-at`) at each layout, so a re-rendered preview never leaves it on a detached element.
- ✅ RESOLVED (web refactor, 2026-10-18) — Web: probe answers are cached per file + position and forgotten whenever
  a file is opened; forgetting now also cancels the probes in flight and drops their late answers. An index that
  goes back to loading after an edit still does not clear the cache (unchanged).
- ✅ RESOLVED (pr-wrap, `CODE_INDEX_STATE_NONE` — see Decisions) — Web: an `IDLE` progress on the pane's session
  reads *Code index starting…*; a worktree with no `Cargo.toml` now gets one `NONE` progress
  (`code_session/without_an_index.rs`) and reads *No code index for this worktree*, so the two are told apart.

## Decisions & Trade-offs

- **Code pane reconnect (VC5) — developer's choice (2026-10-18): auto-reopen with backoff.** When the
  `CodeSession` stream ends or errors while the pane is mounted, `useCodeSession` reopens it after 1 s, 2 s, 4 s …
  capped at 30 s, back to 1 s once a stream has delivered and stayed open ≥ 10 s (FV5 — was: once it delivered any
  event); the strip reads *Reconnecting…* with
  why it dropped; every query still waiting (held or awaiting its answer, probes included) is asked again on the
  new stream with the same request id. Unmounting stops retrying. The queries live on one `CodeSession` object for
  the pane's life, so `useCodeNavigation` never sees the reconnect. **Added in the refactor (flagged for review):**
  a *refusal* — `INVALID_ARGUMENT`, `NOT_FOUND`, `PERMISSION_DENIED`, `UNAUTHENTICATED`, `FAILED_PRECONDITION`,
  `UNIMPLEMENTED` — is not retried (asking again cannot change the host's answer): the strip reads *Indexing failed
  — <reason>* and waiting queries reject. Pinned by `WorktreeCodePaneReconnect.cy.tsx` (7, with `cy.clock`). The
  session header's `WatchCodeIndex` follows the same policy through the shared `StreamReopener` (FV2).
- **Idle eviction while the pane is open — developer's choice (2026-10-18):** the daemon re-warms the root (Rust
  side, VC4), so the pane shows the reload's progress; an `IDLE` progress reads *Code index starting…*, never
  *Code index not loaded* (PRD: "`idle` cannot occur while the pane is open" — as a resting state). See Technical
  Debt for the non-Rust-worktree ambiguity.

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
- **`CODE_INDEX_STATE_NONE` (pr-wrap, 2026-10-18)** — a worktree with no `Cargo.toml` gets one `NONE` progress
  carrying the reason, and the strip reads *No code index for this worktree*. `IDLE` alone could not tell the pane
  "about to load again" (re-warm after eviction) from "will never load", so a non-Rust worktree read *starting…*
  for ever. `code_navigation.proto` only; `code_index` has no such state (the daemon never asks it about a non-Rust
  worktree). Pinned by `a_code_session_on_a_worktree_that_is_not_a_rust_worktree_has_no_index` and
  `CodeIndexStatusStrip.cy.tsx` › *says a worktree that is not a Rust worktree has no code index, and why*.
- **Refusals are not retried** (web reconnect) — a session refused with permission-denied / not-found /
  invalid-argument shows *Indexing failed — reason* rather than retrying; reviewed and kept at pr-wrap.
- **State from the latest `serverStatus`**, not the latch (see State B) — decided while writing the contract; the
  existing navigation tests' fixture now adds `--loads-crate-graph`, and the three tests warm the root to the end
  (`the_graph_of(..).has_loaded()`) before asking — the planning premise that the fake reports quiescence before it
  answers the first request was wrong (it reports ~280 ms after the handshake), so a cold ask correctly reads
  `LOADING`.

- **Idle eviction while the pane is open → re-warm** — developer's choice (2026-10-18). An open `CodeSession`
  counts as using its root: when its watched progress reports `IDLE` after the root held a server since the
  session's last warm, the session calls `code_index.Warm` again; held queries stay held through `IDLE` and are
  answered once `READY`. `FAILED` still answers held queries with `error`. An `IDLE` before the root ever held a
  server (the session's own warm has not reached one yet) starts nothing — a flag, not "is the warm task
  finished", so the rule is deterministic. Fixes VC4 and /validate-prod-ready #5. Since the keep-alive decision
  below, this is the safety net, not the expected path.
- **Keep alive while open** — developer's choice (2026-10-18), resolves FV1 and FV3. An open `CodeSession` is use of
  both the index-daemon **process** and its root's **language server**, so neither is let go while a pane is open.
  `IndexChannelSource` gains `keep_alive()` (the registry refreshes the running process's idle timer; never spawns)
  and `keep_alive_every()` (the registry derives it from its idle timeout: a third of it, clamped to 100 ms–60 s, so
  the same beat also stays well under the index daemon's `A_SERVER_MAY_SIT_UNUSED_FOR` of 600 s). The session calls
  `keep_alive()` on every query and on a heartbeat at that period; the heartbeat also calls `code_index.Warm` when
  the root is `READY`/`LOADING` and no warm of this session is in flight — idempotent (a loaded root answers at once)
  and, through `WorkspaceIndex::client_for`, exactly what records the root's use (`record_use` → `last_used`, and the
  `LspRegistry` tracker via `get_or_spawn`). **No proto RPC added.** An `IDLE` or `FAILED` root is not warmed by the
  heartbeat — starting a load stays the re-warm-on-`IDLE` rule's decision (`code_session/keep_alive.rs`).
  **Trade-off (developer accepted):** a forgotten open tab keeps rust-analyzer — and the index-daemon process —
  resident until the tab closes. Note: `tddy-index-daemon` does not yet call `LspRegistry::reap_idle` (its
  `TODO(M6)` in `main.rs`), so the root half is pre-emptive until that lands. Pinned by
  `an_open_code_session_keeps_the_index_daemon_running_and_lets_it_go_once_closed` and
  `an_open_code_session_keeps_its_loaded_root_in_use_by_warming_it_again` (`code_session_acceptance.rs`, 1 s idle
  timeout configured on the test registry — a real constructor parameter).
- **The header never spawns the index daemon** — developer's choice (2026-10-18). `IndexChannelSource` gains
  `connect_if_running` (`IndexDaemonRegistry::connect_if_running` dials only a process that is already up and does
  not refresh its idle timer); `WatchCodeIndex` uses it and ends with no message when nothing is running. Fixes VC3.
- **A non-Rust worktree has no code index** (VC1): `CodeSession` on a worktree with no `Cargo.toml` neither dials
  the index daemon nor warms/watches; it sends one `NONE` progress (was `IDLE` before the `CODE_INDEX_STATE_NONE`
  decision above) and answers every query with `error: "this
  worktree has no Cargo.toml at its root, so it is not a Rust worktree and has no code index"`
  (`code_session/without_an_index.rs`).
- **`Lsp*` loading note by emptiness** (/validate-prod-ready #3): an empty loading answer says "this empty answer is
  not final"; a loading answer that found something says "this answer may be incomplete".

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
  green removes it from `WorktreeCodePaneProps`. ✅ Done in the web refactor (2026-10-18).
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

Run 2026-10-18 against `71ae6a42c`. Prioritised; overlaps with /validate-prod-ready are cross-referenced, not repeated.

**High**
- ✅ FIXED (daemon side; web "no code index" strip wording is the web agent's) — **VC1** `packages/tddy-daemon-rpc/src/code_session.rs:86-89` (`run`) — `CodeSession` warms **any** listed worktree:
  no `Cargo.toml` gate, unlike `warm_for_session` (`code_index_warmup.rs:36`) and `WatchCodeIndex`
  (`code_navigation.rs`, `.filter(|root| root.join("Cargo.toml").is_file())`). Opening the Code pane on a non-Rust
  worktree now spawns rust-analyzer there (or reports `FAILED`), and the strip shows indexing/failure for a
  worktree navigation is never offered on (`WorktreeCodePane.tsx` gates navigation on `.rs` only). Fix: in `run`,
  when the root has no `Cargo.toml`, skip `Warm`/`WatchRoot`, send one `IDLE` progress (or end the stream with no
  message, as `WatchCodeIndex` does) and answer queries with `error`; pin it with a `code_session_acceptance` test.
  Have the web strip render that state as "no code index for this worktree" rather than "not loaded".
- ✅ FIXED (`cargo fmt -p tddy-daemon -p tddy-lsp-executor -p tddy-daemon-rpc`; `--check` clean) — **VC2** `cargo fmt --check` fails: `packages/tddy-daemon/src/runtime.rs:1188-1192` and
  `packages/tddy-lsp-executor/tests/lsp_tools_via_index.rs:622-623`. Fix: `cargo fmt -p tddy-daemon -p tddy-lsp-executor`.

**Medium**
- ✅ FIXED (`connect_if_running`, see Decisions) — **VC3** `packages/tddy-daemon-rpc/src/code_navigation.rs:253` (`watch_code_index`) — the header's passive watch now
  dials the index daemon via `connect_index` → `IndexDaemonRegistry::connect`, which **starts the index-daemon
  process** on first use. Before this change `WatchCodeIndex` never dialled it. Rust-analyzer is still not started
  (the PRD's "never starts a load" holds), but every session header on a Rust worktree spawns the index daemon.
  Fix: add a non-spawning `IndexChannelSource::connect_if_running` (no process → empty stream, header hidden), or
  record this as accepted in Decisions.
- ✅ FIXED (re-warm on `IDLE`, see Decisions) — **VC4** idle eviction while a pane is open — nothing keeps the root's server used while the `CodeSession` is open,
  so after `A_SERVER_MAY_SIT_UNUSED_FOR` the root reports `IDLE` and the strip reads "Code index not loaded"
  (`CodeIndexStatusStrip.tsx:60`), contradicting the PRD ("`idle` cannot occur while the pane is open"). Same root
  as /validate-prod-ready #5 (held queries never released on `IDLE`, `code_session.rs:352`). Fix both together:
  on `IDLE` while open, re-`Warm` (the pane being open is the demand) and keep held queries held; or amend the PRD
  and answer held queries with an error.
- ✅ FIXED (web refactor, 2026-10-18) — **VC5** `packages/tddy-web/src/components/session/useCodeSession.ts:178-184` — a session that ends or errors
  (LiveKit blip, daemon restart) is never reopened: the strip stays "Indexing failed — the code session ended" and
  every query rejects until the pane remounts. Fix: reopen with backoff on stream end/error (not a fallback — the
  same call again), or show a "Reconnect" action; ask the developer which.
- ✅ FIXED (web refactor, 2026-10-18) — (probe cache moved to `codeProbes.ts` `ProbeStore`; `CodeBlock` memoises the highlighted tree — `HighlightedCode` re-renders only for content / language / target line / theme / file — and each `Identifier` reads its own flag with `useSyncExternalStore`, so a probe answer re-renders that identifier alone) **VC6** `packages/tddy-web/src/components/session/useCodeNavigation.ts:140,331` — every probe answer replaces the
  `probes` map, so `probe`/`dwell`/`isDefinable` change identity and `CodeBlock` re-renders and re-highlights the
  whole file per dwell. Fix: keep the cache in a ref plus a version counter, or memoise the highlighted tree on
  `content` and mark definable spans without re-running the highlighter. (Staleness of in-flight probes is
  /validate-prod-ready #1.)

**Low**
- ✅ FIXED (web refactor, 2026-10-18) — (`Hover.anchor` removed; `positionBeside` looks the identifier up by `data-identifier-at="<line>:<col>"` in the card's containing block at every layout and keeps its place when it is not shown) **VC7** `packages/tddy-web/src/components/session/CodeNavigationOverlays.tsx:71` — the card is positioned from the
  stored `anchor` element; if a re-render replaces the identifier's span (probe answer → class change), the anchor
  is detached and `getBoundingClientRect()` is all zeros, so the card jumps to the preview's top-left. Fix: resolve
  the anchor at layout time from `hover.at` (`worktree-code-identifier-<line>-<col>`), or check `anchor.isConnected`.
- **VC8** `packages/tddy-index-daemon/src/graph.rs:135` — after `NotificationEvent::Lost`, a phase whose `end` was
  lost stays in `RootProgress::phases` for the server's life (stale "+N more" once ready). Fix: drop in-flight
  phases when `serverStatus` turns quiescent (rust-analyzer has ended them by then), or document it.
  ✅ **Fixed (refactor, 2026-10-18)**: `ServerChatter::lost_sight()` marks every token in flight at a gap unconfirmed; a token heard from again is confirmed; the next `quiescent: true` drops the unconfirmed ones (titles kept, so a phase still running reappears named). `graph.rs` calls it on `NotificationEvent::Lost` through a new testable `hear` step. Tests: `graph.rs` `a_phase_whose_end_was_lost_in_a_gap_is_gone_once_the_server_is_quiescent`, `a_phase_that_reported_after_a_gap_is_still_listed_once_the_server_is_quiescent`; `chatter.rs` three gap tests.
- ✅ FIXED (`definition_of` helper, separate `Definition` / `Probe` arms) — **VC9** `packages/tddy-daemon-rpc/src/code_session/ask.rs:97` — `Kind::Definition | Kind::Probe` then re-matches
  `query.kind` inside; split into a `definition_of` helper used by two arms for readability.
- **VC10** `.claude/settings.json` — unrelated allow-list change (`Read(~/.claude/projects/**)`); same as
  /validate-prod-ready #6.
- ✅ FIXED (final /validate-changes, doc only) — **VC11** Changeset State B snippet (`code_index.proto` / `code_navigation.proto`) omits `FAILED` and
  `IndexProgress.error` that the Decisions section added — snippet now carries `FAILED`, `NONE` and `error`.

### From /validate-tests

Run 2026-10-18. Prioritised; `R*` Rust, `W*` web. None critical.

**High**
- ✅ FIXED — added `a_held_hover_on_an_index_that_then_fails_is_answered_with_the_failure`, `a_definition_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready`, `references_asked_while_the_index_loads_are_held_and_answered_once_it_is_ready`, `a_probe_where_nothing_is_defined_answers_none`, `a_query_reusing_a_held_query_s_id_replaces_it` (the handler defines reuse as replacement), `a_code_session_whose_index_stops_following_its_worktree_ends_unavailable`, `a_code_session_opened_with_a_token_nobody_holds_is_refused`, plus `an_index_let_go_while_the_pane_is_open_is_warmed_again`, `a_hover_asked_after_the_index_was_let_go_is_held_until_it_is_ready_again`, `a_code_session_on_a_worktree_that_is_not_a_rust_worktree_has_no_index`. The fake now reports `FAILED` as `FAILED`, answers references, knows only the call to `foo`, and can stop following (`stops_following`) — **R1** `packages/tddy-daemon/tests/code_session_acceptance.rs` — `CodeSession` branches with no test: a held query on a root that reports `FAILED` (answered `error: "the code index failed: <reason>"`); a held **definition**/**references** (only hover is held); a probe answering `DEFINABILITY_NONE`; a reused `request_id` replacing the earlier query; `WatchRoot` ending → session `UNAVAILABLE`; a bad `session_token`. Prerequisite: `support/mod.rs:389-396` (`AScriptedIndex::state`) maps `FAILED` to `LOADING` and always answers `foo` once ready — give it a failed state and a symbol without a definition.
- ✅ FIXED — added `watch_code_index_while_no_index_daemon_runs_ends_with_nothing_and_starts_none`, `watch_code_index_for_a_session_whose_worktree_is_not_created_yet_ends_with_nothing`, `watch_code_index_of_a_worktree_without_a_cargo_manifest_ends_with_nothing`, `watch_code_index_without_an_index_daemon_section_ends_with_nothing`, `watch_code_index_of_a_session_whose_record_is_unreadable_is_a_failed_precondition`; renamed the warm test to `a_session_s_rust_worktree_handed_to_the_warm_up_is_warmed_through_the_index_daemon`. The watch tests that follow a root now start the index daemon first (`AnIndexDaemonHost::a_running_registry`) — **R2** `packages/tddy-daemon/tests/code_index_warmup_acceptance.rs` — the "ends with no message" paths of `WatchCodeIndex` (worktree not created yet, no `Cargo.toml`, no `index_daemon:`) and the unreadable-record `FAILED_PRECONDITION` are untested since `watching_a_session_nothing_warmed_ends_at_once_with_nothing` was removed. `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists` (:135) implies a "not yet" case that does not exist — add it or rename.
- **R3** `packages/tddy-index-daemon/src/watch_root.rs:219` (`reported`) / `src/graph.rs:52` (`RootProgress::fold`) — no unit test pins the "state from the latest `serverStatus`, not the latch" decision: `LOADING` before any status, and `READY` → `LOADING` when a quiescent server goes busy again. Add unit tests beside the moved `graph.rs` ones.
  ✅ **Fixed**: `graph.rs` `a_root_whose_server_has_sent_no_status_reads_loading`, `a_ready_root_reads_loading_again_when_its_server_goes_busy` (asserts `LOADING` with `loaded` still latched).
- ✅ FIXED (web refactor, 2026-10-18) — (`host.failLoading(reason)` + `anIndexFailed`; strip "names the reason when the index fails to load", "a held go-to-definition reports that the index failed to load, and why") **W1** `cypress/support/rpc/codeSessionBackend.ts:225-258`, `CodeIndexStatusStrip.cy.tsx:46-61` — the fake cannot report a root `FAILED` on an open stream nor answer a held query with `error`; `useCodeIndexProgress.ts:39-40,48` FAILED branch is unreached. Add `host.failLoading(reason)` + tests "names the reason when the index fails to load" and "a held go-to-definition reports the index failure".
- ✅ FIXED (web refactor, 2026-10-18) — (`host.queriesCancelled()`; "closing a loading hover card cancels its query on the host", "moving to another identifier cancels the held hover of the one before") **W2** `WorktreeCodePaneCodeSession.cy.tsx` — no test that abandoning a held hover sends `cancel` (`useCodeNavigation.ts:59-67,298-313`). Record cancels in the fake (`host.cancelledQueries()`), add "leaving a held hover cancels it on the host".
- ✅ FIXED (web refactor, 2026-10-18) — ("a probe the loading index cannot answer leaves the identifier plain and is asked again once it is ready") **W3** `WorktreeCodePaneCodeSession.cy.tsx:182-211` — the `UNKNOWN` probe's result (identifier stays plain, not cached, re-probed after ready) is not asserted.
- ✅ FIXED (web refactor, 2026-10-18) — ("probes a position once however often the pointer rests on it", "a plain click on an identifier without a definition asks nothing") **W4** `WorktreeCodePaneCodeSession.cy.tsx:259-302` — the per-position probe cache (one PROBE for two dwells) and a plain click on a non-definable identifier (no DEFINITION sent) are untested.

**Medium**
- **R4** `packages/tddy-index-daemon/tests/root_progress_acceptance.rs:448-456` — `watch.next()` assumes the very next message is `FAILED`; `refused.is_err()` and `!after.error.is_empty()` are loose. Use `watch.until("failed", …)`, assert the status code, and assert the reason text (or a stable fragment, with a comment).
  ✅ **Fixed**: `watch.until_failed()`; refusal asserted `(Unavailable, "lsp server exited")` and the watch's `error` equal to it (`A_SERVER_THAT_EXITS_AT_ONCE`). Observed: this test takes ~10 s — `tddy-lsp` notices a server that exits at startup only when `initialize` hits `DEFAULT_REQUEST_TIMEOUT` (10 s); pre-existing, out of this scope.
- ✅ FIXED (re-ask count assertion dropped) — **R5** `code_session_acceptance.rs:339-346` — the held-hover test also pins the re-ask count (an implementation detail) — a third behaviour beyond its name. Drop it or move it to its own test.
- ✅ FIXED (web refactor, 2026-10-18) — ("renders a fenced rust block as one code block, apart from the docs after it", "renders everything after a fence that is never closed as code"; `hoverBodyCodeBlocks` on the page object) **W5** `CodeHoverCard.cy.tsx:138-152` — `renderSimpleMarkdown` fenced code blocks (every rust-analyzer hover) and the unclosed-fence path (`renderSimpleMarkdown.tsx:55`) are untested; add "renders a fenced rust block as code, not paragraphs".
- ✅ FIXED (web refactor, 2026-10-18) — (`codePreview.{dwellOn, goToDefinitionOf, plainClick, listReferences, closeHoverCard}` in the pane driver, used by the drawer spec) **W6** `WorktreeCodePaneNavigation.cy.tsx:127-129,157-159,172-174` — inline `.click({ ctrlKey: true })` / `.trigger("mouseover")`; expose `dwellOn` / `goToDefinitionOf` as shared helpers.
- ✅ FIXED (web refactor, 2026-10-18) — (`cypress/support/rpc/codeSessionAssertions.ts` — `expectTheHost(host).toHaveOpened / toHaveBeenAsked / toHaveBeenAskedOfKind / toHaveCancelled / toHaveOpenAndEndedSessions`) **W7** `WorktreeCodePaneNavigation.cy.tsx:138-145` — inline `cy.wrap(null).should(...)` duplicates `expectTheHost*` from `WorktreeCodePaneCodeSession.cy.tsx:81-110`; move them into `codeSessionBackend.ts` (or `codeSessionAssertions.ts`) as fluent assertions.
- ✅ FIXED (web refactor, 2026-10-18) — (`cypress/support/fixtures/geometryWorktree.ts`, incl. `aGeometryWorktreeHost(hover?)`) **W8** fixtures (`MAIN_RS`, `THE_CALL_TO_AREA`, `GEOMETRY_RS`, `THE_DEFINITION_OF_AREA`, `INDEXING_38`, …) copied in `CodeHoverCard.cy.tsx:22-33`, `WorktreeCodePaneCodeSession.cy.tsx:35-63`, `WorktreeCodePaneNavigation.cy.tsx:52-77` → `cypress/support/fixtures/geometryWorktree.ts`.
- ✅ FIXED (web refactor, 2026-10-18) — (strip spec now pins only failed / idle-reads-starting / reconnecting) **W9** `CodeIndexStatusStrip.cy.tsx:25-44` duplicates `WorktreeCodePaneCodeSession.cy.tsx:126-154`; keep the strip spec on strip-only concerns.
- ✅ FIXED (web refactor, 2026-10-18) — **W10** `CodeIndexStatusStrip.cy.tsx:57-60` — `contain.text` where the headline is exact: `have.text "Indexing failed — index daemon unreachable"`.

**Low**
- **R6** `packages/tddy-code-restructuring/src/backends/rust/chatter.rs` (`track_in_flight`) — untested: a second `begin` on an in-flight token restarts it; a lower percentage after a higher one keeps the higher (only the `None` case is pinned).
  ✅ **Fixed** (tests only; code already right): `a_second_begin_on_a_running_token_restarts_its_phase_in_begin_order`, `an_active_phase_keeps_its_highest_percentage_when_a_lower_one_follows`.
- **R7** `packages/tddy-index-daemon/src/graph.rs` tests (moved verbatim from `warm.rs`) — `furthest.contains(..)` / `line.contains(..)` loose asserts and assertion roulette; pre-existing, assert exact strings when next touched.
  ✅ **Fixed**: exact `furthest` / lost-sight strings, one tuple assertion per test.
- **R8** `root_progress_acceptance.rs:33-62` — two host builders differ only in `args`; fold into one `a_host_running_fake_lsp(args)`. (The copy of `unary_at`/`a_message_carrying`/the workspace fixture is already listed under @red.)
  ✅ **Fixed**: `a_host_running_fake_lsp(args)`; the two named hosts are one-line wrappers.
- ✅ FIXED (`the_index_s_call_to_foo()`; `THE_SERVER_WENT_AWAY` const) — **R9** `code_session_acceptance.rs:267` hardcodes `SourcePosition { line: 2, column: 5 }` instead of `the_call_to_foo()`; `code_index_warmup_acceptance.rs:239,247` repeats the failure reason — make it a const.
- ✅ FIXED for the daemon (`AN_EVENT_IS_DUE_WITHIN` 30 s → 5 s, justified: the first event waits on the registry starting the stand-in `sh` process; tests take ~0.2–2 s each). ⏸ DEFERRED: `A_PROGRESS_STEP` in `tddy-index-daemon` — that package is being restructured — **R10** `packages/tddy-daemon/tests/support/mod.rs:49` — `AN_EVENT_IS_DUE_WITHIN` = 30 s, above the 1 s integration ceiling, while its comment says every event is a local round-trip; lower it (e.g. 5 s) or justify the figure. Same for `A_PROGRESS_STEP` (10 s) in `root_progress_acceptance.rs:26`.
  ◐ **Partly fixed**: `A_PROGRESS_STEP` lowered 10 s → 5 s with the justification in its doc. `AN_EVENT_IS_DUE_WITHIN` (`tddy-daemon`) left to the agent owning that package.
  **Final /validate-changes sync**: as committed, `AN_EVENT_IS_DUE_WITHIN` is **15 s**, not 5 s (`support/mod.rs:54`) — its doc records that 5 s timed out 2–9 of 17 code-session tests on a loaded host. Both figures are justified in place; R10 closed.
- ✅ FIXED (module-wide allow removed; each item one suite does not use carries `#[allow(dead_code)] // <suite> only`) — **R11** `support/mod.rs:19` — crate-wide `#![allow(dead_code)]`; prefer per-item allows so dead helpers surface.
- ✅ FIXED (asserts code and message) — **R12** `code_session_acceptance.rs:454` asserts only `InvalidArgument`, not the message.
- **R13** `packages/tddy-lsp/tests/bin/fake_lsp.rs:57` — "All three are off unless asked for" is stale (many modes now).
  ✅ **Fixed**: comment now says every mode above is off unless asked for.
- **R14** naming: `root_progress_acceptance.rs`, `code_session_acceptance.rs` use the `_acceptance` suffix the fluent-tests guide rejects; it is the repo-wide convention and `test_placement.rs` names these files, so leave it for a repo-wide rename.
- ✅ FIXED (web refactor, 2026-10-18) — **W11** `WorktreeCodePaneCodeSession.cy.tsx:169-175` — tooltip `title` checked with four `include`s; assert the exact string (pins begin order).
- ⏸ DEFERRED — optional; the long tests are deliberate timelines (loading → progress → ready), each step a Given/When/Then; new behaviours were given their own tests instead — **W12** `WorktreeCodePaneCodeSession.cy.tsx:126-154,182-211` — ~4 behaviours per test; split, or note them as deliberate timelines.
- ✅ FIXED (web refactor, 2026-10-18) — (sync points commented in place; the reconnect spec syncs on the strip naming each failed attempt) **W13** `WorktreeCodePaneCodeSession.cy.tsx:265-267`, `CodeIndexStatusStrip.cy.tsx:50` — assertions used as sync points inside Given/When; make them waiting driver verbs or comment them.
- ✅ FIXED (web refactor, 2026-10-18) — (callback assertion — `not.have.attr(name)` changes the chained subject) **W14** `WorktreeCodePaneCodeSession.cy.tsx:276-279` — `not.have.attr("data-definable","true")` also passes for other values; use `not.have.attr("data-definable")`.
- ✅ FIXED (web refactor, 2026-10-18) — (query without a position ends the session `INVALID_ARGUMENT`; undeclared file is `NOT_FOUND`; the unknown-symbol hover answering no markdown is kept and documented — it is rust-analyzer's real behaviour, and `MAIN` relies on it) **W15** `codeSessionBackend.ts:277-278` (missing `position` → 0), `:314-316` (undeclared file → `""`), `:411` (unknown-symbol hover → `undefined`, no spec) — fake tolerates bad input silently; throw instead.
### From /validate-prod-ready
### From /analyze-clean-code

Prioritised for a refactor agent. **(new)** = introduced or grown past a threshold by this branch;
**(pre)** = pre-existing in a touched file — report only, fix only if cheap. Line numbers are the
working tree on 2026-10-18 (`71ae6a42c` + uncommitted changes).

**Must refactor — introduced by this change (P1)**

- **CC1** (new) `packages/tddy-daemon-rpc/src/code_session/ask.rs:70-131` `ask_index` — length **62** (target ≤40).
  The Hover / Definition / References arms repeat `if loading(state) { Reply::Loading } else { Reply::Answer(..) }`.
  Fix: add `hover_of` / `references_of` beside `definition_of` (`:134`), and a
  `fn settled(state, answer: impl FnOnce() -> Answered) -> Reply`; build the answers with shared
  converters (see CC11), so each arm is ~3 lines.
  ✅ **Fixed** (Rust refactor 2026-10-18): `ask_index` 33 lines — one `PositionQuery` (`code_navigation/position_query.rs`), `settled(state, || ..)` and the shared `web_*` converters; each arm 3 lines.
- **CC2** (new) `packages/tddy-index-daemon/src/watch_root.rs:136-164` `follow` — nesting **5–6**
  (loop → match → arm → `select!` → arm → `if`; target ≤3). Fix: extract
  `async fn wait_for_roots_change(roots_changed: &mut watch::Receiver<()>, events) -> bool` (the
  `select!` at `:153-160`), mirroring `forward`, so the `Unheld` arm becomes
  `if !sent.send_if_new(..).await || !wait_for_roots_change(..).await { return; }`.
  ✅ **Fixed**: `follow` 19 lines, nesting 2 — the `Unheld` arm is `send_if_new(..) && next_change(&mut roots_changed, events).is_ok()` (the generic `next_change` of CC15 is the `wait_for_roots_change`).
- **CC3** (new) — **FIXED** (refactor, 2026-10-18): one `NavigationContext` (`navigationLookup.ts`); split into `useDefinitionLookup.ts` (46 lines), `useHoverQuery.ts` (42), `useReferencesLookup.ts` (15) over the shared `useLookup` (32). Was: `packages/tddy-web/src/components/session/useCodeNavigation.ts:134-274`
  `useNavigationActions` — length **141**, **6** params (target ≤40, ≤4). Fix: pass one
  `{ session, selectedPath, openFile, ui, probes, queries }` context object (or move `openFile`/`probes`
  into `ui`), and split into `useDefinitionLookup` (`navigate` + `goToDefinitions` + `openLocation`),
  `useHoverQuery` (`showHover`) and `useReferencesLookup` (`showReferences`). Fold the shared lookup
  shape into one helper first (CC14), which removes ~30 lines.
- **CC4** (new) — **FIXED**: `useCodeNavigation` 34 lines — composition only; `useQueries`, `useClearNavigation`, `useProbeDwell(probes, selectedPath, showHover)` extracted. Was: `packages/tddy-web/src/components/session/useCodeNavigation.ts:283-343` `useCodeNavigation` —
  length **61** (just over). Fix: once CC3 splits the actions, return `{ ...actions, selected, openFile, … }`
  rather than re-listing each field (`:330-342`); or move `clear` + `dwell` + `definable` into a
  `useProbeDwell(probes, selectedPath, showHover)` hook.
- **CC5** (new) — **FIXED**: `class CodeSessionFollower { start(); stop() }` (`follow` / `stream` / `deliver` / `scheduleReopen`, each ≤13 lines, nesting ≤3); `useCodeSession` 22 lines, effect is create → `start` → `return stop`. Cancellation unchanged: `stopped` flag, `channel.close()` ends the request iterable, then `abort`. Was: `packages/tddy-web/src/components/session/useCodeSession.ts:217-286` `useCodeSession` —
  length **70**, nesting **5** (effect → `follow` → `try` → `for await` → `if`). Fix: lift the
  reconnect loop out of the effect into a plain class or function, e.g.
  `class CodeSessionFollower { start(); stop() }` holding `stopped` / `abort` / `reopen` / `attempt`,
  with `follow()` and `scheduleReopen(reason)` as methods; the effect becomes create → `start` →
  `return stop`.
- **CC6** (new) — **FIXED**: `readHeading` / `readFence` / `readList` / `readParagraph` + `readBlock`; `renderSimpleMarkdown` 15 lines; keys are `blocks.length` (same sequence as the old counter). Was: `packages/tddy-web/src/components/session/renderSimpleMarkdown.tsx:18-97`
  `renderSimpleMarkdown` — length **80** (60 at master; heading + fence blocks added), nesting **4**.
  Fix: one reader per block kind — `readHeading`, `readFence`, `readList`, `readParagraph`, each
  `(lines, i) => { node, next } | null` — and a loop that tries them in order. Rename `L`, `t`, `j`
  (`:67-68`, `:80`) and `el` (`:21`) while there (CC24).
- **CC7** (new) — **FIXED**: `useCardPosition(cardRef, at)`, `HoverCardBody`, `HoverCardActions` extracted; `HoverCard` 32 lines. Was: `packages/tddy-web/src/components/session/CodeNavigationOverlays.tsx:59-129` `HoverCard` —
  length **71** (31 at master), mostly markup. Fix: extract `useCardPosition(cardRef, hover.at)` (the
  `useLayoutEffect` + state at `:70-82`), `HoverCardBody` (`:97-108`) and `HoverCardActions` (`:109-126`).

**Must refactor — pre-existing in a touched file (P3, report only)**

- **CC8** (pre) `packages/tddy-daemon/src/runtime.rs:658` `build` — ~1047 lines, nesting 6. This branch
  *shrank* it (−9). Already recorded: `packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md`.
- **CC9** (pre, moved verbatim) `packages/tddy-index-daemon/src/plan_flush.rs:43-93` `flush_eventually` — nesting **5**
  (loop → for → match → arm → `if`), 51 lines. Fix: extract
  `fn report_outcomes(reported: &mut BTreeMap<PathBuf, String>, outcomes)` (`:76-91`) and
  `fn flush_unlocked(stores) -> Vec<(PathBuf, Result<(), String>)>` (the `spawn_blocking` body, `:61-72`).
- **CC10** (pre) `packages/tddy-web/src/components/session/CodeBlock.tsx` — `CodeBlock` `:279-346` length **68**
  (65 at master); `renderNode` `:110-140` **7** params; `renderText` `:142-172` **5** params.
  Fix: bundle `line`, `cursor`, `stylesheet`, `useInlineStyles`, `identifiers` into one `RowContext`
  passed to both; move the `actions` ref wiring (`:290-324`) into `useIdentifierActions(onNavigate, onHover)`.
- **CC10b** (pre) `packages/tddy-web/src/components/session/WorktreeCodePane.tsx:50-146` `WorktreeCodePane` — length **97**,
  unchanged by this branch. `packages/tddy-web/src/components/session/useCodeIndexProgress.ts:77-110`
  `useCodeIndexProgress` — nesting **5** (effect → async IIFE → `try` → `for await` → `if`), unchanged structure.

**Duplication (P2)**

- **CC11** `packages/tddy-daemon-rpc/src/code_session/ask.rs:79-129` and
  `packages/tddy-daemon-rpc/src/code_navigation.rs:144-234` — the same three index calls and the same
  `DefinitionResponse` / `ReferencesResponse` / `HoverResponse { state: web_state(..) as i32, .. }`
  conversions, built 5 times. Fix: `pub(crate) fn web_definition(index::DefinitionResponse)`,
  `web_references`, `web_hover` next to `web_progress` (`code_navigation.rs:434`), used by both; the
  unary methods also share a `Forward::ask_hover/definition/references`.
  ✅ **Fixed**: converters moved to `packages/tddy-daemon-rpc/src/code_navigation/convert.rs` (+ `web_definition` / `web_references` / `web_hover` / `web_locations`); the three index calls are `PositionQuery::{definition,references,hover}`, used by the unary methods (via `Forward::into_position_query`, 21 lines each) and `ask.rs`. `code_navigation.rs` 492 → 433 production lines.
- **CC12** `packages/tddy-daemon-rpc/src/code_session.rs:118-152` `start_warm` (nesting 4) and
  `packages/tddy-daemon-rpc/src/code_index_warmup.rs:50-73` `warm` — two drainers of one `Warm`
  stream. Fix: one `pub(crate) async fn drain_warm(client, workspace_root) -> Result<(), String>` in
  `code_index_warmup.rs`; `start_warm` spawns it and logs the `Err` — also removes the nesting.
  ✅ **Fixed**: `pub(crate) async fn drain_warm(stream) -> Result<bool, tonic::Status>` in `code_index_warmup.rs`; `warm` and `start_warm` both drain through it (the `Warm` request itself stays awaited in `start_warm` before `WatchRoot`, as before). `start_warm` nesting 2.
- **CC13** `packages/tddy-daemon-rpc/src/code_session.rs:238-261` `on_frame` (nesting 4) and
  `packages/tddy-daemon-rpc/src/code_session/without_an_index.rs:40-58` `serve` (nesting 4) — the `Open`
  / `None` rejections and their strings are duplicated. Fix: `fn reject_frame(frame: &Option<Frame>) -> Status`
  (or two consts `OPENED_ONCE`, `NOT_A_FRAME`) in `code_session.rs`.
  ✅ **Fixed**: `fn misplaced_frame(&Option<Frame>) -> Status` in `code_session.rs`, matched by one `misplaced @ (Some(Frame::Open(_)) | None)` arm in both `on_frame` and `without_an_index::serve_frame`; strings unchanged. `on_frame` split into `on_frame` + `on_query` (nesting 2); `without_an_index::serve` loop body extracted to `serve_frame`.
- **CC14** — **FIXED** (with CC3): `useLookup(...)(which, relPath, at, onFound)` in `navigationLookup.ts`. Was: `packages/tddy-web/src/components/session/useCodeNavigation.ts:173-207` `navigate` and `:245-271`
  `showReferences` — same shape: ask → `onHeld` sets the held notice → `track` → clear notice →
  error notice → case check → "not loaded" notice when empty and not `READY`. Fix:
  `askLookup(kind, relPath, at, label, onFound)` inside the hook.
- **CC15** `packages/tddy-index-daemon/src/watch_root.rs:176-197` `forward` and
  `packages/tddy-index-daemon/src/warm.rs:102-127` `forward_until_loaded` — same send → `select!`
  (`progress.changed()` vs `events.closed()`) loop. Fix: one
  `async fn next_change(progress, events) -> Result<(), Gone>` with `Gone::{Server, Caller}`, used by both.
  ✅ **Fixed**: `pub(crate) async fn next_change<T, E>(&mut watch::Receiver<T>, &EventSender<E>) -> Result<(), Gone>` with `Gone::{Server, Caller}` in `watch_root.rs`, used by `forward`, `follow` and `warm.rs` `forward_until_loaded` (now `-> Result<RootProgress, Gone>`; `Followed` and `warm::Ending` removed).
- **CC16** `packages/tddy-index-daemon/src/warm.rs:52-53`, `:65-66`, `:80-81` — `activity.refused(&r); let _ = events.send(Err(r)).await;`
  three times in `serve_warm` (55 lines, needs attention). Fix: `async fn refuse(activity, events, refusal)`.
  ✅ **Fixed**: `async fn refuse(activity, events, refusal)`; the spawned body moved to `async fn warm_root(index, root, events, activity)` (41 lines) — `serve_warm` is 9.
- **CC17** `packages/tddy-index-daemon/src/navigation.rs:index_state_of` and
  `packages/tddy-index-daemon/src/watch_root.rs:236-240` — `if quiescent { Ready } else { Loading }` twice.
  Fix: `fn state_of_quiescence(quiescent: bool) -> IndexState` in `graph.rs`.
  ✅ **Fixed**: `pub(crate) fn state_of_quiescence(bool) -> IndexState` in `graph.rs`, used by `navigation.rs` `index_state_of` and `watch_root.rs` `reported`.
- **CC18** `packages/tddy-lsp-executor/src/index_backed.rs:241-283` `navigate` (43 lines) — the Definition
  and References arms carry identical `.map(|answer| { let answer = answer.into_inner(); (answer.state(), answer.locations) })`.
  Fix: return the raw response from each arm via a small `trait LocationsAnswer` or two one-line
  helpers, then map once.
  ✅ **Fixed**: a private `trait LocationsAnswer { fn into_state_and_locations(self) }` for `DefinitionResponse` / `ReferencesResponse`; both arms `.map(|answer| answer.into_inner().into_state_and_locations())`.
- **CC19** `err instanceof Error ? err.message : String(err)` in
  `useCodeSession.ts:201`, `useCodeNavigation.ts:39` (`errorMessage`), `useCodeIndexProgress.ts:63`.
  Fix: one exported `messageOf(err)`.

**Magic values (P2)**

- **CC20** `"Cargo.toml"` as the "is a Rust worktree" test in `code_session.rs:96`, `code_navigation.rs:253`,
  `code_index_warmup.rs:36` (and `tddy-lsp-executor/src/index_backed.rs:78`). Fix:
  `pub(crate) fn is_rust_worktree(root: &Path) -> bool` in `code_navigation.rs`, used by the three daemon-rpc sites.
  ✅ **Fixed**: `pub fn tddy_index_daemon::is_rust_workspace(root)` + `RUST_WORKSPACE_MANIFEST` in `packages/tddy-index-daemon/src/lib.rs` — the common dependency of `tddy-daemon-rpc` and `tddy-lsp-executor`, and the crate that defines what it serves; used at all four sites.
- **CC21** `"index daemon: {err}"` prefix at `code_navigation.rs:132`, `:262`, `code_index_warmup.rs:54`;
  `"warm: {}"` at `code_index_warmup.rs:58`, `:64` — one `fn index_daemon_unavailable(err)` / const prefix.
  ✅ **Fixed**: `index_daemon_error(&IndexChannelError) -> String` / `index_daemon_unavailable(..) -> Status` in `code_navigation.rs` (3 sites); `warm_failed(status)` for the two `"warm: {}"` in `code_index_warmup.rs`. `tddy-lsp-executor`'s single `"index daemon: {err}"` left as is (one site, other crate).
- **CC22** — **FIXED** (with CC3): labels named once — `LOOKUPS` in `navigationLookup.ts`, `HOVER_LABEL` in `useHoverQuery.ts`. Was: `useCodeNavigation.ts` — `"Go to definition"` (`:193`, `:203`), `"Hover"` (`:227`, `:238`),
  `"Find references"` (`:257`, `:269`) each written twice; name them once per action (falls out of CC14).
- **CC23** `chatter.rs` — the `"begin"` progress kind literal in `progress_at` (`:187`) and
  `track_in_flight` (`:257`); a `const BEGIN` (or a parsed `ProgressKind` enum) keeps the two readings
  from drifting. `CodeNavigationOverlays.tsx:50` subtracts `rem` while `:93` caps width at
  `calc(100% - 2rem)` — tie the two margins to one named constant.
  ✅ **Rust half fixed**: `const BEGIN` in `chatter.rs` (3 production readings). The `CodeNavigationOverlays.tsx` margin is the web half.

**Naming (P3)**

- **CC24** — **FIXED** (with CC6): `el`/`L`/`t`/`j` → `blocks.length` key / `itemLine` / `item` / `itemIndex`. Was: `renderSimpleMarkdown.tsx:21,67-68,80` — `el`, `L`, `t`, `j` → `nextKey`, `itemLine`, `item`, `index`.
- **CC25** `code_session.rs:170` `held_since_warmed` — reads as "a query was held"; it means "the root
  reported a non-`IDLE` state since the last warm". Rename `served_since_warmed`. `Session::release`
  (`:379`) also re-holds — `settle_held` says what it does.
  ✅ **Fixed** (Rust): `held_since_warmed` → `served_since_warmed`; `Session::release` → `settle_held`.
- **CC26** `WorktreeCodePane.tsx` — `const code = useCodeSession(..)` → `codeSession`.

**Oversized files (> 500 production lines) — all pre-existing, all already recorded under `packages/*/docs/code-issues/`**

- `packages/tddy-daemon/src/runtime.rs` 1822 (−9 here) — `oversized-file-runtime.md`; split later.
- `packages/tddy-code-restructuring/src/backends/rust.rs` 521 (1-line re-export change) — `oversized-file-backends-rust.md`; leave.
- `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` 674 (−1 here) — `oversized-file-session-main-pane.md`; leave.
- `packages/tddy-index-daemon/proto/code_index.proto` 601 (**+49 here**, schema not code) — leave; split
  only if a second progress service appears.
- ✅ ~~**Watch:**~~ (now **433** after CC11) `packages/tddy-daemon-rpc/src/code_navigation.rs` **492** (+88 here) will cross 500 on the
  next change. CC11 is the natural seam: move `web_*` / `index_position` converters (`:422-481`) into
  `code_navigation/convert.rs` (~70 lines; `ask.rs` and `code_session.rs` import from there).

## Validation Results

### Daemon-side refactor (2026-10-18)

Scoped to `tddy-daemon-rpc`, `tddy-daemon`, `tddy-lsp-executor` (never workspace-wide):
`code_session_acceptance` 17/17 (6 repeated runs, no flakes), `code_index_warmup_acceptance` 11/11,
`code_navigation_acceptance` 7/7, `plan_dialog_acceptance` 5/5, `test_placement` 4/4; `tddy-daemon-rpc
--no-fail-fast` 46 targets, 0 failed; `tddy-lsp-executor` all pass (`lsp_tools_via_index` 15/15);
`cargo clippy -p tddy-daemon-rpc -p tddy-lsp-executor -p tddy-daemon --all-targets -- -D warnings` clean;
`cargo fmt --check` clean. `runtime.rs` 1822 production lines (master 1831); touched non-test files all under 500.

### /validate-changes

- **Last run**: 2026-10-18 against `71ae6a42c` (`git diff master...HEAD`, 53 files; not a stack branch — base `master`, no PR).
- **Status**: ⚠ Pass with warnings — 0 critical, 6 warnings (2 high, 4 medium), 5 info; details under Refactoring
  Needed → From /validate-changes.
- **Build (scoped)**: `cargo build -p tddy-code-restructuring -p tddy-index-daemon -p tddy-service -p tddy-daemon-rpc
  -p tddy-lsp-executor -p tddy-lsp -p tddy-daemon` ✅ (only pre-existing `fake_lsp` output-filename-collision
  warnings); `cargo clippy … --all-targets -- -D warnings` ✅; `cargo fmt --check` ❌ 2 files (VC2).
- **Tests (scoped, run here)**: all green — see Scope → Testing. Cypress not re-run.
- **Changeset sync**: Protos 🔲→✅ (+🆕 `FAILED`/`error`); "Index state on navigation answers" ⚠→✅ (the three
  `code_index_service_acceptance` tests now pass); "`code_navigation.proto` + TS codegen" 🔲→✅; Technical Debt
  "three navigation tests … left for the test author" → ✅ resolved; Testing / Code quality scope lines refreshed;
  TODO "TDD Green" → ✅.
- **Requirements alignment**: every PRD acceptance criterion has code behind it; two diverge — "`idle` cannot occur
  while the pane is open" (VC4) and the implicit "only Rust worktrees warm" (VC1).
- **Security**: `CodeSession` authorises `open` with `resolve_listed_worktree`; each query's `rel_path` goes through
  `validate_rel_path_shape`; `WatchCodeIndex` authorises with `resolve_owned_session_dir` before reading the record.
  No secrets, no `unsafe`, no stdout in TUI paths, no test-only branches.

#### Final (pr-wrap step 5)

- **Run**: 2026-10-18 against `888740395` (`git diff master...HEAD`, 70 files; ordinary branch — base `master`, no
  PR, no stack headings). Read-only review: no build or test run here (the scoped gates run in a separate process);
  `cargo fmt --check -p` on the seven touched Rust packages ✅ clean.
- **Status**: ⚠ Pass with warnings — **0 critical**, 3 warnings (1 high, 2 medium), 6 info. Nothing blocks on
  correctness, security or build; **FV1 is a behaviour regression the developer should fix or explicitly accept
  before merge.**
- **Refactor round introduced** (new since `71ae6a42c`):
  - ⚠ **FV1 (high)** `packages/tddy-daemon-rpc/src/code_session.rs:101,325-329` — a `CodeSession` dials the index
    daemon **once** (`connect_index` at open) and asks every query over that one client. The index-daemon
    *process*'s idle timer is refreshed only by `IndexDaemonRegistry::get_or_spawn` → `live()`
    (`packages/tddy-daemon/src/index_daemon/registry.rs:162-171`), so a pane in active use stops counting as use
    after open. Before this branch the pane's unary navigation called `connect_index` per request and kept the
    process alive. With nothing else dialling (no agent tool call, no session start), the process is reaped
    `idle_timeout_secs` (default 30 min) after the pane opened → `WatchRoot` ends → session `UNAVAILABLE` → the web
    reconnects → `connect` respawns → a **cold rust-analyzer reload** (6–10 min on this workspace) under a pane
    being used. No test pins it. Fix options: refresh the registry's idle timer per query (e.g. an
    `IndexChannelSource::touch`, or `connect` per query as the unary path does), or count an open `CodeSession` as
    use for its life.
    ✅ **Fixed (2026-10-18, keep alive while open — see Decisions):** `IndexChannelSource::keep_alive` /
    `keep_alive_every` (`IndexDaemonRegistry::keep_alive` refreshes the running process's idle timer without
    spawning; the beat is a third of the idle timeout, clamped to 100 ms–60 s); the session calls it on every query
    and on a heartbeat while open (`code_session/keep_alive.rs`). Pinned by
    `an_open_code_session_keeps_the_index_daemon_running_and_lets_it_go_once_closed` (1 s idle timeout: not stopped
    over 2.5 s open, stopped once closed); both new tests fail with the heartbeat disabled.
  - ⚠ **FV2 (medium)** `packages/tddy-daemon-rpc/src/code_navigation.rs` `watch_code_index` +
    `packages/tddy-web/src/components/session/useCodeIndexProgress.ts:84-107` — VC3's `connect_if_running` makes
    the header's watch end empty when no index daemon is up, and `useCodeIndexProgress` never re-subscribes (deps
    are client / token / session id only). A header mounted before the index-daemon process starts — host restart,
    or after the process's idle stop — never shows the load the pane or the agent then starts until it remounts.
    This narrows PRD criterion *"The agent's first `Lsp*` / `restructure_*` call on a cold worktree makes the
    header indicator show live progress"* to hosts whose index daemon is already running. Same for a header whose
    `WatchRoot` ended when the process stopped. Fix: re-subscribe on stream end (with backoff, like the pane), or
    record the narrowed criterion in the PRD.
    ✅ **Fixed (2026-10-18, web):** `useCodeIndexProgress` now follows the watch with a `CodeIndexWatcher` that
    re-watches after every end or retryable failure, on the pane's schedule (shared `StreamReopener`,
    `src/components/session/streamReopening.ts`: 1 s doubling to 30 s); a refusal is not re-watched and reads
    *Indexing failed — <reason>* (now without the `[code]` prefix, as the pane does); unmount stops it. Pinned
    by `SessionIndexingIndicatorResubscribe.cy.tsx` (*shows a load that starts after its first watch ended
    empty*, *watches again after 1, 2 and 4 seconds*, *a watch the host refuses is not asked again*). Cost: a
    header on a host with no index daemon (or a non-Rust worktree) asks one cheap empty `WatchCodeIndex` every
    30 s once backed off, so a later load can take up to 30 s to show there.
  - ⚠ **FV3 (medium)** re-warm on `IDLE` (VC4 fix, `code_session.rs:262-266,359-369`) — an open but unused pane
    (a forgotten tab) keeps rust-analyzer cycling: evicted after `A_SERVER_MAY_SIT_UNUSED_FOR` (600 s,
    `tddy-index-daemon/src/main.rs:60`), re-warmed at once, a full cold load, evicted again 600 s after it — for
    as long as the tab is open; and FV1 restarts the whole process every 30 min on top. The developer chose
    re-warm; this cost is not recorded in Decisions. Alternatives: an open session keeps its root's `last_used`
    fresh (no reload), or re-warm lazily on the next query instead of on `IDLE`.
    ✅ **Fixed (2026-10-18, keep alive while open — see Decisions):** the session's heartbeat warms a `READY` /
    `LOADING` root again (idempotent; reaching the server is the index daemon's `record_use`), so an open pane's root
    is not let go; re-warm on `IDLE` stays as the safety net. The cost — a forgotten tab keeps rust-analyzer
    resident until it closes — is recorded in Decisions as accepted. Pinned by
    `an_open_code_session_keeps_its_loaded_root_in_use_by_warming_it_again`.
  - ℹ **FV4** `packages/tddy-daemon-rpc/src/code_session.rs:20-22` — module doc says a non-Rust worktree is
    reported "`IDLE` once"; the code (`code_session/without_an_index.rs:21-31`) sends `NONE`. Doc-comment fix.
    ✅ **Fixed (2026-10-18):** the module doc now says it is reported "`NONE` once with why".
  - **FV1/FV3/FV4 fix gates (2026-10-18, scoped):** `cargo check -p tddy-daemon-rpc -p tddy-daemon -p
    tddy-index-daemon --all-targets` ✅; `code_session_acceptance` 19/19 ✅; `cargo clippy -p tddy-daemon-rpc -p
    tddy-daemon --all-targets -- -D warnings` ✅; `cargo fmt` on the three packages ✅. Full run left to CI.
  - ℹ **FV5** `packages/tddy-web/src/components/session/useCodeSession.ts:268-273` — `attempt` resets to 0 on
    any delivered event, so a host that opens the session, sends its first progress and then drops (e.g. an
    index daemon crashing on start) is retried every ~1 s indefinitely, each retry dialling `connect` (which spawns).
    Documented as the decided behaviour; worth a cap or a "delivered for N s" rule if it shows up.
    ✅ **Fixed (2026-10-18, web):** the waits start from 1 s again only once a stream has delivered **and** stayed
    open ≥ 10 s (`STABLE_STREAM_MS` in `streamReopening.ts`) — chosen over "after READY" so a long, healthy load
    that drops mid-way still reconnects fast. The reconnect spec's *a reopened session that delivered waits a
    second again* depended on the old rule (it dropped the reopened session at once); it now keeps the session
    open 10 s first and is renamed accordingly. New: *a host that accepts each session and drops it at once is
    backed off, not asked every second* (1, 2, 4 s).
  - ℹ **FV6** `useCodeIndexProgress.ts:92-99` — a header watch that ends after it has heard something keeps its
    last status; a stream that ends mid-load (index daemon gone) leaves *Indexing — X n%* until remount.
    ✅ **Fixed (2026-10-18, web):** a watch that ends reports `idle` (no indicator) until the next watch reports;
    one that fails retryably shows its reason until then. Pinned by *the header drops what it last said once its
    watch ends mid-load*.
- **Carried open (not regressions)**: VC10 / prod-ready #6 `.claude/settings.json` allow-list line unrelated to
  the feature; CC19 (`err instanceof Error ? …` at `useCodeSession.ts:203`, `useCodeIndexProgress.ts:63`), CC23 web
  half (`CodeNavigationOverlays.tsx:50` `rem` vs `:153` `2rem`), CC26 (`WorktreeCodePane.tsx:61` `const code`);
  CC8–CC10b pre-existing; W12, R14 deferred. Package docs still describe `SessionIndexProgress` /
  `with_index_progress` / *not warmed* (`tddy-daemon-rpc/docs/architecture.md`, `tddy-daemon/docs/code-navigation-service.md`,
  `tddy-daemon/docs/daemon-endpoint.md`, `tddy-web/docs/session-start-and-indexing-progress.md`,
  `docs/ft/web/session-code-pane.md:117`) — wrap's job, as planned. PRD *What's Changing* still gives the `Lsp*`
  loading example with a percentage (*Indexing 38%*), which the acceptance-test review dropped.
- **`TODO(code-index-warm-on-demand)`**: none in the tree. New `TODO`/`FIXME`/`unimplemented!`/`todo!`/debug
  output in the diff: none (the one `println!` is inside a Cypress fixture string).
- **Changeset claims checked against code**: every listed acceptance test exists by name (Rust: 17 + 11 + 7 + 3
  `lsp_tools_via_index` loading/answer tests + chatter/graph units; Cypress: 19 + 6 + 5 + 4 + 3); State B matches
  the protos once VC11 was applied (this run); `connect_if_running` does not refresh the idle timer, as claimed;
  `misplaced_frame`, `drain_warm`, `next_change`/`Gone`, `state_of_quiescence`, `is_rust_workspace`,
  `convert.rs`, `position_query.rs`, `plan_flush.rs` (index.rs 458) all present as described. Fixed-finding
  markers verified for VC1–VC9, R1–R13, W1–W15, CC1–CC7, CC11–CC18, CC20–CC22, CC24, CC25.
- **Changeset sync (this run)**: Scope Testing / Code-quality lines refreshed (stale `7/7`, `6/6`, fmt ❌);
  State B snippets gain `FAILED` / `NONE` / `error` (VC11 ✅); Technical Debt "IDLE for a non-Rust worktree — open
  question" → ✅ resolved by `NONE`; Decisions "sends one `IDLE`" → `NONE`; R10 corrected (`AN_EVENT_IS_DUE_WITHIN`
  is 15 s, not 5 s); prod-ready #2's fix note moved off #6; strip spec list gains the `NONE` test; TODO checklist
  synced.

### /validate-tests

- **Last run**: 2026-10-18 — static review against fluent-tests (generic + `rust/std-test.md` + `typescript/cypress-component.md`); no tests run.
- **Status**: ⚠ Pass with warnings — 0 critical, 29 warnings (7 high, 9 medium, 13 low); details and fixes under Refactoring Needed → From /validate-tests.
- **Tests analyzed**: 51 — Rust 35 (`root_progress_acceptance` 7, `code_session_acceptance` 7, `code_index_warmup_acceptance` 6, `lsp_tools_via_index` 1 new, `code_index_service_acceptance` 3 and `code_navigation_acceptance` 3 modified, `chatter.rs` 5 new, `graph.rs` 3 moved); Cypress 16 (`WorktreeCodePaneCodeSession` 8, `CodeHoverCard` 3, `CodeIndexStatusStrip` 2, `WorktreeCodePaneNavigation` 3).
- **Clean on the hard rules**: no `#[ignore]`, `.only`, `.skip`, `cy.wait(ms)`, `cy.intercept`, raw `cy.get`/`data-testid` in specs, or sleeps in tests (the `fake_lsp` signal file replaces them); Given/When/Then throughout; every stream await bounded; RPC via `mountWithRpc` + `anInMemoryRpcBackend` and in-memory `code_index` fakes.
- **Anti-patterns found**: missing edge/error cases (R1–R3, W1–W5 — chiefly the `FAILED` root / failed held query path and `cancel`, untested at the daemon `CodeSession` and web layers); loose matchers (R4, R7, W10, W11, W14); multiple behaviours per test (R5, W12); duplicated fixtures/helpers (R8, W7, W8, W9); timeouts above budget (R10); fakes tolerating bad input (W15).
- **Description-body mismatches**: 1 — `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists` (`code_index_warmup_acceptance.rs:135`) implies a "not yet" case it does not test. `a_hover_asked_while_the_index_loads_is_held_and_answered_once_it_is_ready` also pins the re-ask count (R5).
- **Critical issues**: none.
### /validate-prod-ready

**Last run:** 2026-10-18 against `71ae6a42c` (`git diff --name-only master...HEAD`) — **Status: ⚠️ Gaps** (0
blockers, 6 warnings, 6 acceptable/informational).

- Files: 26 production sources checked (+ the 2 proto contracts read); 21 test / Cypress / `src/gen` / `gen/` files
  excluded; 3 docs and `.claude/settings.json` noted separately.
- Verification (**scoped**, never workspace-wide): `cargo check -p tddy-index-daemon -p tddy-daemon-rpc
  -p tddy-lsp-executor -p tddy-code-restructuring -p tddy-daemon` — clean, no warnings;
  `cargo clippy -p tddy-index-daemon -p tddy-daemon-rpc -p tddy-lsp-executor -p tddy-code-restructuring
  -- -D warnings` — clean. Tests not re-run by this step; web `tsc` is not a gate (pre-existing errors).

| Category | Count | Status |
|----------|-------|--------|
| Mock code | 0 | ✅ |
| Dev fallbacks | 0 (1 documented compat branch, acceptable) | ✅ |
| TODO/FIXME | 1 new (known) | ⚠️ |
| Unused code | 2 | ⚠️ |
| Debug output | 0 (no `println!`/`eprintln!`/`dbg!`/`console.*`; logging via `log::`) | ✅ |

**Warnings (should fix, not blocking):**

1. ✅ FIXED (web refactor, 2026-10-18) — (`ProbeStore` tracks probes in flight; `forget` cancels them on the host and bumps a generation so late answers are dropped; pinned by "opening another file cancels the probes in flight, and their late answers underline nothing") `packages/tddy-web/src/components/session/useCodeNavigation.ts:116-147` (`useProbes`) — probes are not tracked or
   cancelled: `forget()` (run on every file open, including `openLocation` within the same file) clears the cache
   and `asked`, but an in-flight probe's `settle` still runs afterwards and re-inserts its key, so a re-read file can
   underline from a stale answer; the host also keeps answering probes for a file no longer shown. Fix: keep the
   probe handles (or a generation counter bumped by `forget`) and `session.cancel` / drop outcomes from an older
   generation.
2. `packages/tddy-index-daemon/src/watch_root.rs:257` — `standing_of` folds the unreachable `RootView::Held(_)` into
   the `Idle` arm, so if it were ever reached it would report a held root as `IDLE`. Fix: give `standing_of` a
   type without `Held` (e.g. split `RootView` into `Held(..)` vs `Standing::{Idle, Failed}`), so the compiler
   proves it.
3. ✅ FIXED (note by emptiness, see Decisions; `lsp_references_found_while_the_index_loads_are_marked_possibly_incomplete` added, the empty case's JSON unchanged) — `packages/tddy-lsp-executor/src/index_backed.rs:280-281` — `THE_INDEX_IS_STILL_LOADING` says "this empty
   answer", but it is attached to every `LOADING` answer, including a non-empty hover or a partial location list.
   Fix: reword ("this answer may be incomplete…") or attach only when the result is empty — say which in the doc.
4. ✅ FIXED (web refactor, 2026-10-18) — (prop removed, with `SessionMainPane.tsx`'s line and the driver's) `packages/tddy-web/src/components/session/WorktreeCodePane.tsx:38` — `TODO` with no tracking reference
   (known, intentional; the `sessionId` prop is unused). Fix: remove the prop with `SessionMainPane.tsx` and the
   Cypress driver, or give the TODO a `docs/dev/todo/` reference like `index_backed.rs:192` has.
5. ✅ FIXED (re-warm on `IDLE`; the `_` arm keeps a query held through `IDLE` by design, now documented and pinned by `a_hover_asked_after_the_index_was_let_go_is_held_until_it_is_ready_again`) — `packages/tddy-daemon-rpc/src/code_session.rs:352` (`release`, `_` arm) — a held query stays held when the root
   reports `IDLE` (server let go) or `UNSPECIFIED`; only `READY`/`FAILED` release it, so such a query is never
   answered until the pane closes while the UI shows "Index loading". Fix: on `IDLE`, answer held queries with an
   error (or re-ask, accepting that it re-spawns the server) — and pin it with a test.
   ✅ **#2 fixed (refactor, 2026-10-18)**: `RootView::{Held(_), Unheld(Standing)}` with `Standing::{Idle, Failed(String)}`; `standing_of` takes a `Standing`, so a held root cannot reach it. Both enums are now module-private (no other user).
6. `.claude/settings.json` — the branch adds `"Read(~/.claude/projects/**)"` to the allow list; unrelated to the
   feature (agent harness config). Fix: confirm it is wanted in this PR or drop it from the branch. **Still open at
   final /validate-changes.**

**Acceptable / informational:**

- `packages/tddy-web/src/components/session/useCodeIndexProgress.ts:47` — `UNSPECIFIED` read from `error`/`ready`
  for a host that predates `state`: a compatibility branch planned in State B (this changeset), not a silent
  fallback.
- `packages/tddy-web/src/components/session/useCodeIndexProgress.ts:30` — `"the code index failed"` when a
  `FAILED` carries no `error`: a display placeholder, does not mask the failure.
- `packages/tddy-index-daemon/src/watch_root.rs:50` — `RootSignals.failures` is pruned only when a root is served
  again; a root that only ever failed stays recorded (bounded by distinct roots asked). Fine for now.
- `packages/tddy-index-daemon/src/graph.rs:52,147` — `RootProgress::fold` clones the phase list and `how_far()`
  on every notification to compare; cheap at rust-analyzer's rates, worth revisiting only if profiling says so.
- `packages/tddy-lsp-executor/src/index_backed.rs:192` — pre-existing `TODO(docs/dev/todo/…)`, referenced.
- Changeset hygiene: Technical Debt (“three navigation tests… Left for the test author”) contradicts Decisions
  (“the three tests warm the root to the end”) — reconcile before wrap.

### /analyze-clean-code

- **Last run**: 2026-10-18 against `71ae6a42c` + uncommitted working tree (`git diff master`); static
  analysis only, no build or tests. Rust production lines counted up to the first `#[cfg(test)]`.
- **Files**: 33 production sources — 20 Rust (`tddy-code-restructuring`, `tddy-daemon-rpc`, `tddy-daemon`,
  `tddy-index-daemon`, `tddy-lsp-executor`), 13 web (`tddy-web/src/components/session*`) — plus the 2
  protos for file length. Excluded: tests, Cypress, `src/gen`, `gen/`, docs, `.claude/settings.json`,
  `buildId.ts`, the unrelated terminal changeset.
- **Overall score: F** — 7 "must refactor" items introduced or grown by this branch (CC1–CC7), plus 6
  pre-existing in touched files (CC8–CC10b). Counts: **critical 13** (must refactor), **major 14** (needs
  attention), **minor 16** (duplication / magic values / naming: CC11–CC26).
- **Function metrics** (164 functions in touched/new code: 94 Rust, 70 TS):

  | Metric | Excellent | Acceptable | Needs Attention | Must Refactor |
  |--------|-----------|------------|-----------------|---------------|
  | File length | — | 31 | 4 (all pre-existing; the proto grew +49) | — |
  | Function length | 111 | 38 | 6 | 9 |
  | Nesting depth | 138 | 12 | 9 | 5 |
  | Parameter count | 153 | 6 | 3 | 2 |

- **Priority fixes (introduced here)**: `code_session/ask.rs:70` `ask_index` 62 lines; `watch_root.rs:136`
  `follow` nesting 6; `useCodeNavigation.ts:134` `useNavigationActions` 141 lines / 6 params;
  `useCodeNavigation.ts:283` `useCodeNavigation` 61 lines; `useCodeSession.ts:217` `useCodeSession` 70
  lines / nesting 5; `renderSimpleMarkdown.tsx:18` 80 lines; `CodeNavigationOverlays.tsx:59` `HoverCard`
  71 lines. Details, fixes and the duplication list under Refactoring Needed → From /analyze-clean-code.
- **Needs attention (touched)**: `chatter.rs:187` `progress_at` 56 (53 at master); `warm.rs:34`
  `serve_warm` 55 (pre); `index.rs:264` `client_for` 46; `index_backed.rs:241` `navigate` 43;
  `CodeBlock.tsx:217` `HighlightedCode` 53 / nesting 4; nesting 4 in `code_session.rs` `start_warm` `:118`,
  `serve` `:207`, `on_frame` `:238`, `without_an_index.rs:22` `serve`, `watch_root.rs:176` `forward`,
  `warm.rs:102` `forward_until_loaded`; 5 params in `code_navigation.rs:97` `authorise_and_connect`
  (pre), `navigation.rs:90` `locations_at` (pre), `CodeBlock.tsx:142` `renderText` (pre).
- **Method note**: metrics from a brace/indent scan of each function plus a manual read of every
  flagged one; Rust struct-literal braces were discounted by hand. TSX components count markup lines,
  which is why `HoverCard` / `CodeBlock` / `WorktreeCodePane` cross 60.
- Scope checkbox "Code quality" left unticked (score below B).

## Restructuring (pr-wrap 3.5)

This PR grew `packages/tddy-index-daemon/src/index.rs` from 522 to **532** production lines (+10: the
`signals: RootSignals` field, its import and initialiser, the `signals()` accessor, and the two call sites in
`client_for`/`record_use`). The
developer chose to offset the growth by moving one cohesive seam out, engine-driven
(`tddy-tools restructure`, warm index daemon), behaviour-preserving — no moved code written by hand.

**Seam:** the plan-store plumbing the code-issues record already names — the debounced write-back of
loaded plans. **Module layout after:**

| Module | Holds |
|---|---|
| `index.rs` | `WorkspaceIndex`, `RootState`, `REQUEST_BOUND_ABOVE_ANY_COLD_INDEX`, `read_tree`, `described` (unchanged) |
| `plan_flush.rs` (new, private `mod plan_flush`) | `PLAN_FLUSH_DEBOUNCE`, `PLAN_FLUSH_TICK`, `SharedPlanStore`, `PlanStores`, `flush_eventually` |

**Plans** (two, because `check --deep` resolves each op against the tree as it stands and cannot see a module
an earlier op of the same plan creates):

1. `move_item` `{PLAN_FLUSH_DEBOUNCE, PLAN_FLUSH_TICK, SharedPlanStore, PlanStores}` with `name: "plan_flush"`,
   `to: "tddy_index_daemon"`, `reexport: "none"` — callers in `apply.rs`, `plan_upkeep.rs`, `index.rs` re-pointed
   to `crate::plan_flush::SharedPlanStore`; the engine widened three private items to `pub(crate)`.
2. `move_item` `{flush_eventually}` to `tddy_index_daemon::plan_flush`, `reexport: "none"` — widened to
   `pub(crate)`.

Each: `check --deep` → no findings; `apply --dry-run` → resolved 1 of 1; `apply` → applied 1 of 1 (compile gate
passed). No refusal, no hand edits.

| | Production lines |
|---|---|
| `index.rs` at master | 522 |
| `index.rs` after this PR's feature work | 532 |
| `index.rs` after the move | **458** |
| `plan_flush.rs` | 84 (whole file, no tests) |

After the refactor pass (2026-10-18): `plan_flush.rs` gained a `//!` module doc (93 lines); `index.rs` untouched at 458 production lines. (Its flusher still logs under target `tddy_index_daemon::index`, kept so existing log filters keep matching.)

`SharedPlanStore` lost its old public path `tddy_index_daemon::index::SharedPlanStore` (no consumer outside the
crate named it). Gate after the move (scoped): `cargo fmt -p tddy-index-daemon --check` ✅,
`cargo clippy -p tddy-index-daemon --all-targets -- -D warnings` ✅, `cargo test -p tddy-index-daemon
--no-fail-fast` ✅ 15 suites, 169 passed, 12 ignored — identical to the baseline before the move. The statement
multiset of `index.rs` at `HEAD` equals `index.rs` + `plan_flush.rs` after, modulo `use` lines and visibility.

## TODO

- [x] Record initial discovery (`2026-10-18-code-index-warm-on-demand-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved 2026-10-18, with the three recommendations)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (scoped locally, full on CI) — verify 100% pass
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation — VC1–VC7, VC9, VC11 fixed; VC8 fixed; VC10 open (developer's call)
- [ ] USER REVIEW — development complete
- [x] Validate tests (/validate-tests)
- [x] Refactor test issues — all but W12 (deferred, deliberate timelines) and R14 (repo-wide naming)
- [x] Validate production readiness (/validate-prod-ready)
- [~] Refactor production readiness issues — #1–#5 fixed; #6 (`.claude/settings.json`) open
- [x] Analyze code quality (/analyze-clean-code)
- [~] Refactor code quality issues — CC1–CC7, CC11–CC18, CC20–CC22, CC24, CC25 fixed; CC19, CC23 (web half), CC26 open; CC8–CC10b pre-existing, report only
- [x] Final validation (/validate-changes) — 2026-10-18 against `888740395`, see Validation Results
- [ ] Linting and formatting (scoped `cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-18-code-index-warm-on-demand-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
