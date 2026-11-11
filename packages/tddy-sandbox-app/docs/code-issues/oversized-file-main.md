# oversized-file: main.rs

**Location:** `packages/tddy-sandbox-app/src/main.rs`
**Category:** oversized-file
**Detected:** 2026-10-22 by `/pr-wrap` step 3.5 file-length gate on PR #670 (`#provider-route` 9/9)
**Metrics:** **957 production lines** (2026-11-03, after #799) (counted to the first `#[cfg(test)]`) · budget 500
**Thresholds breached:** length 976 > 500
**Restructure:** required — not designed, `/code-restructuring` territory
**Status:** Open — **unclaimed**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-22 | 976 | first record; 973 at PR #670's merge-base (`655970c72`), already over budget. +3 in #670: the `--agent-kind` doc and the `agent_home_banner` arm for the two kinds the app does not start. |
| 2026-11-03 | 957 | **better** by 19 in PR #799 (`#agent-tools` 10/12): `--agent <spec id>` replaces `--agent-kind`, `--claude-binary`, `--cursor-binary`, `--claude-home-dir` and `--cursor-home-dir`. The Linux path refuses an agent other than claude (+9) |
**Worse by 1:** 957 → 958 production lines (#825, `#agent-tools` 19/19): the banner names a `home: { kind: path }` home (`HomeSection::Path`). Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer. **Status:** Open — regressed 2026-11-06 by #825.

## What the tool found

The awk count to the first `#[cfg(test)]`, the one `/pr-wrap` step 3.5 uses. Complexity, nesting and
CRAP were not measured (no `analyze coverage` run: the disk on the measuring host was too tight for an
instrumented build).

## Why it matters here

The file was over budget before #670 and is a little further over now. Every PR that adds a few lines
here looks fine in review, which is how a file gets this size.

## What would close it

Not designed. `main.rs` holds the CLI `Args` and the run loop; the banner and home-directory helpers are the likely first seam.

## Verified by hand

2026-10-22 — Confirmed both counts with the gate's awk, and read #670's diff of the file. Not
decomposed in #670: the orchestrator scoped that PR's decomposition to files it pushed past 500.

**Regressed 2026-11-11 by #890 (`#usage-binding` 1/5):** 966 → **973 production lines** (+7): the two `print_token_summary` call sites build a `tddy_usage::UsageContext`. Decomposition attempted at wrap with `tddy-tools restructure` (`move_item` of `host_home` + `print_token_summary` into a new `token_summary` module): `check --deep` reported no findings, but `apply` declared the module in **`lib.rs`** — the engine resolves `tddy_sandbox_app::` to the library root even for an anchor in the binary's `main.rs` — and the tree no longer compiled (`E0433 unresolved crate tddy_sandbox_app` in the moved file). Rolled back; not moved by hand. Engine defect: `move_item` cannot target a binary crate root in a package that also has a `lib.rs`. **Status:** Open — regressed 2026-11-11 by #890.
