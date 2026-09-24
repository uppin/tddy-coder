# oversized-file: connection_service.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518; **missed by the file-length gate**
**Metrics:** ~**1,634 production lines** of 1,647 total · budget 500 · **3.3× over**
**Thresholds breached:** length ~1634 > 500
**Restructure:** `extract_module --to_file` — seam not yet designed
**Status:** Open — partially fixed 2026-09-23 by #520 (~1,931 → ~1,634; still 3.3× over), then regressed +5 production lines by #508 — **unclaimed**
**Verified:** ⚠ the automated count is **wrong for this file** — see below

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | ~1930 | first detection; +87 in PR #518 |
| 2026-09-22 | ~1931 | unchanged in substance — total 1,944 → 1,945; #494 (`#carve` 8/11) swapped the `telegram` field for `presenter_event_sink` and its doc comment, +1 line net |
| 2026-09-23 | ~1634 | #520 (`#carve` 11/12): total 1,945 → 1,647 (−298). The free functions only the four moved RPC families used left for `tddy-daemon-rpc` — `merge_listed_projects_with_peers`, `require_pr_stack_orchestrator`, `owner_repo_from_repo_root`, the `base_sync_*` helpers, the `agent_models_cache` group, the path guard and result framing — with four test-module declarations. Same method as the rows above (lines before the trailing test-module declaration, `:1635`). Excluding every `#[cfg(test)]` item and its doc comment: 1,859 → 1,569 |
| 2026-09-23 | ~1639 | total 1,647 on `origin/master` (`4e260d7f`, after #520) → 1,652 after #508 (`#keyring` 1/9): the `session_tokens: Option<SessionTokens>` field on `DaemonSessionHost` and its doc comment — all five lines production, none test. Split deferred to a follow-up after `#keyring` lands, since dependents #509–#513 touch this file |
| 2026-09-24 | ~1639 | **unchanged by #510** (`#keyring` 3/9), which touched it: total 1,652 on `origin/master` (`35cf2913`) → 1,652. The `DaemonSessionHost.github_token_store` field and its three-line doc comment became `credential_vaults: Option<Arc<tddy_daemon_auth::SessionVaults>>` with a three-line doc comment — four lines replaced by four |

## The number has to be taken by hand

`/pr-wrap` step 3.5 counts production lines to the **first** `#[cfg(test)]`. This file puts
`#[cfg(test)] use` declarations at **line 44**, for imports only its extracted test modules need, so
the automated count exits there and reports **43**. The real test module starts at `:1931`.

**The mod-aware cut undercounts it too.** Cutting at the first `#[cfg(test)]` that opens a `mod` — the
rule `tddy-daemon-rpc`'s shape tests use — stops at `#[cfg(test)] mod stack_child_spawn_tests;` at
`:817` and reports **816** (808 on master), because this file interleaves out-of-line test-module
declarations with production items that continue to `:1618`. Only a count that excludes each
`#[cfg(test)]` item individually is right for it.

That is why this file has no earlier record despite being one of the largest in the crate. See
[`docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md`](../../../../docs/dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md).

## What the file holds

A facade plus the service struct: `MpscResultStream` (`:64-117`), `DaemonSessionHost` (`:127`), and
**40+ `mod` declarations** wiring in `connection_service/`'s submodules. Most behaviour already
lives in those submodules. What remains here, as measured 2026-09-23:

- the struct's fields and its re-exports;
- two large free functions, `spawn_claude_cli_session_inner` (`:263-670`, ~400 lines) and
  `prepare_managed_workflow_inner` (`:709`, ~100 lines);
- the placement vocabulary (`CodebasePlacement`, `PlacementRequest`, `classify_placement`,
  `classify_codebase_placement`, `resolve_split_agent_placement`, `~:1011-1265`);
- smaller helpers: conversation-spawn slugs, attachment cleanup, `activity_delta_frames`,
  `validate_stack_seed_base_session`, `session_worktree_source`, `sandbox_claude_passthrough_args`.

`:38-43` stack `#[cfg(test)]` attributes with nothing between them (`#[cfg(test)] #[cfg(test)] use …`),
residue of earlier `use` removals (present on master before #520). Harmless to the compiler; worth
removing whenever the file is next edited.

## What would close it

The four RPC families' free functions have left (#520). What remains is ~1,134 lines over budget.
Seams, none proven:

- **`spawn_claude_cli_session_inner`** — ~400 lines in one free function; the largest single cut
  available, into its own `connection_service/` module beside `svc_start_claude_cli_session`.
- **The placement vocabulary** — `CodebasePlacement`, `PlacementRequest`, `classify_placement`,
  `classify_codebase_placement` and their tests are a cohesive ~120 lines with no dependency on
  `DaemonSessionHost`'s fields. `extract_module --to_file` as `codebase_placement.rs`.
- **`MpscResultStream`** — a general stream adapter that names nothing in this crate.
- `#carve`'s successor nodes (split-session and sandboxed-start cluster; session agents, rosters and
  clones) move further `connection_service/` code out of the crate, but not this file's own items;
  none of them closes this record by itself.

The residue is `DaemonSessionHost`'s field list and constructor, which is what a composition root
looks like and may be the right size once the rest leaves.
