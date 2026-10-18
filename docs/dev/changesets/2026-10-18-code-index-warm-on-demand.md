# 2026-10-18 — Code index warms on demand, with live LSP progress
**Type:** Feature

The code index's progress is a per-worktree-root fold of rust-analyzer's own notifications that anyone
can watch without starting a load; the Code pane opens one bidirectional `CodeSession` that warms its
worktree, streams that progress live and carries hover / definition / references / probe queries, held
while the index loads. Before this, the index warmed only at some session starts, progress was an
in-memory per-session record (so a restarted host, or a sandboxed, tool or split session, read *Code
index not warmed for this session*), and navigation answered empty while rust-analyzer loaded.

- **`tddy-code-restructuring`** — `ServerChatter::active_phases` (`ActivePhase`) and `lost_sight`.
  [readiness-and-gates.md](../../../packages/tddy-code-restructuring/docs/readiness-and-gates.md).
- **`tddy-index-daemon`** — `GraphLoad` folds the root's progress for the server's whole life
  (`RootProgress`); `WatchRoot` (never starts a server; `IDLE` / `LOADING` / `READY` / `FAILED`); `Warm`
  forwards the root's progress; `Definition` / `References` / `Hover` carry `IndexState`;
  `is_rust_workspace`. `plan_flush.rs` split out of `index.rs`.
  [code-index-service.md](../../../packages/tddy-index-daemon/docs/code-index-service.md#root-progress).
- **`tddy-service`** — `code_navigation.proto`: `CodeSession` and its frames, `CodeIndexState` (incl.
  `FAILED`, `NONE`), `CodeIndexPhase`, state on the three answers.
  [code-navigation-proto.md](../../../packages/tddy-service/docs/code-navigation-proto.md#the-code-session).
- **`tddy-daemon-rpc`** — `CodeSession` handler (warm, follow, hold, cancel, probe, keep-alive, no-index
  worktrees); `WatchCodeIndex` follows `WatchRoot` passively via `IndexChannelSource::connect_if_running`;
  `SessionIndexProgress` and `with_index_progress` removed; `code_index_warmup` is a trigger only.
  [architecture.md](../../../packages/tddy-daemon-rpc/docs/architecture.md#codesession).
- **`tddy-daemon`** — `IndexDaemonRegistry::{connect_if_running, keep_alive, keep_alive_every}`; wiring.
  [code-navigation-service.md](../../../packages/tddy-daemon/docs/code-navigation-service.md).
- **`tddy-lsp-executor`** — `Lsp*` answers given while loading carry `"index": "loading"` and a note.
  [index-backed-executor.md](../../../packages/tddy-lsp-executor/docs/index-backed-executor.md).
- **`tddy-lsp`** — `fake_lsp --loads-crate-graph-on-signal`.
- **`tddy-web`** — `useCodeSession` (reopen with backoff), strip with every phase, held queries, probe
  underline + plain click, bounded floating hover card, header re-watches `WatchCodeIndex`.
  [code-navigation.md](../../../packages/tddy-web/docs/code-navigation.md),
  [session-start-and-indexing-progress.md](../../../packages/tddy-web/docs/session-start-and-indexing-progress.md).

Product: [Session code pane](../../ft/web/session-code-pane.md),
[Session drawer § Start progress](../../ft/web/session-drawer.md#start-progress).

**Backlog entries resolved and deleted:**

- *The code-index warm is followed by session record, so a restarted host forgets it*
  (`2026-10-18-code-index-warm-is-followed-by-session-record`) — progress is per worktree root and
  `WatchCodeIndex` / `CodeSession` follow `WatchRoot`; opening the Code pane warms.
- *Navigation answers empty while the index is still loading*
  (`2026-10-18-navigation-answers-empty-while-the-index-loads`) — `IndexState` / `CodeIndexState` on the
  three answers; `CodeSession` holds a query until `READY`.

**Backlog entry narrowed:** *Start phases and the code-index warm skip the sandboxed, tool and split
starts* (`2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts`) — the warm
half is answered (warming is on demand); the start-phase half stays.

**Code issues closed (final measurements, then deleted):**

- `tddy-index-daemon` `oversized-file-index` — `src/index.rs` **458** production lines (to the first
  `#[cfg(test)]`; budget 500). 522 on `master`; +10 for the `RootSignals` field, accessor and two call
  sites (532), then −74 by an engine-driven move (`restructure` `move_item` ×2, `reexport: none`, no hand
  edits) of the plan write-back seam — `PLAN_FLUSH_DEBOUNCE`, `PLAN_FLUSH_TICK`, `SharedPlanStore`,
  `PlanStores`, `flush_eventually` — into the private module `plan_flush.rs` (93 lines with its module
  doc). `cargo test -p tddy-index-daemon --no-fail-fast` identical before and after the move (15 suites,
  169 passed, 12 ignored).
- `tddy-index-daemon` `complexity-warm-narrate-until-loaded` — `narrate_until_loaded` (44 lines, nesting
  5) is **deleted**; `Warm` forwards the root's progress (`forward_until_loaded` over the shared
  `next_change`, nesting ≤ 2). `warm.rs` 196 → 128 production lines.

**Code issues re-measured, kept:** `tddy-daemon` `oversized-file-runtime` 1,831 → **1,822** (−9);
`complexity-runtime-build` `build` 1,056 → **1,047** (−9, brace matching) — the `SessionIndexProgress`
wiring removed; `tddy-web` `oversized-file-session-main-pane` 675 → **674** (the `sessionId` prop
removed); `tddy-code-restructuring` `oversized-file-backends-rust` **3,025 unchanged** (one re-export
line); `tddy-index-daemon` `poisoned-warm-latch-on-interrupted-index` unchanged (latch moved to
`RootProgress::loaded`; mechanisms 2–3 open).

**Deferred to the backlog:** in-jail `Lsp*` tools never reach the host index
(`2026-10-18-lsp-tools-in-a-sandbox-jail-never-reach-the-host-index`); the hover card does not follow
preview scroll (`2026-10-18-code-hover-card-does-not-follow-preview-scroll`); `tddy-lsp` `initialize`
waits 10 s on an exited server (`2026-10-18-tddy-lsp-initialize-waits-out-its-timeout-on-an-exited-server`);
reconnect specs' retried "not reopened yet" checks
(`2026-10-18-reconnect-specs-check-no-early-reopen-with-a-retried-assertion`); clean-code leftovers
CC19, CC23 (web), CC26 and test-support duplication (`2026-10-18-code-pane-clean-code-leftovers`).

**Accepted trade-offs:** a forgotten open Code pane keeps rust-analyzer and the index daemon resident
until it closes; a header on a host with no index daemon re-asks an empty `WatchCodeIndex` every 30 s
once backed off, so a later load can take up to 30 s to show there.
