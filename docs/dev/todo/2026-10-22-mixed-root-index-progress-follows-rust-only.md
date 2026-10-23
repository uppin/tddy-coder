# 2026-10-22 — a root served by Rust and an extension language reports Rust's progress only

**Category:** Behaviour gap
**Found in:** `#reshape` 22/24 (`extension-language-server`, PR #681), at `/green`
**Owner:** unassigned — `tddy-index-daemon`

## What happens

`WorkspaceIndex::record_use` (`packages/tddy-index-daemon/src/index.rs`) keeps **one** progress watcher
(`GraphLoad`) per root. When a root holds both rust-analyzer and an extension language's server, Rust's watcher wins:
an extension server's watcher is replaced by Rust's, never the other way round. `WatchRoot`, `Warm` and the session's
index state therefore report rust-analyzer's load alone; an extension server that fails or exits while Rust's is
healthy is not reflected in the root's state.

## Why it was not fixed in #681

R8 defined readiness per launch, and every acceptance test of #681 is an extension-only worktree or a Rust-only one.
A per-language state is a schema question: `RootProgress` and `code_index.IndexState` describe one server per root.

## Wanted

Either a per-language progress (a `RootProgress` per `LspKey`, folded into one root state by a stated rule — e.g.
`FAILED` if any failed, `LOADING` while any loads), or the rule written down that a mixed root reports Rust's
progress. Then a `root_progress_acceptance.rs` case over a root holding both kinds of server.
