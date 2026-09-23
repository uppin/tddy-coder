# oversized-file: log_backend.rs

**Location:** `packages/tddy-log/src/log_backend.rs`
**Category:** oversized-file
**Detected:** 2026-09-23 — `/pr-wrap` step 3.5 file-length gate on #522 (`#carve` 12/14), production lines counted to the first `#[cfg(test)]`
**Metrics:** **862 production lines** of 1,055 total (`#[cfg(test)]` at line 863; 193 test lines) · budget 500 · **1.7× over**
**Thresholds breached:** length 862 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Moved:** 2026-09-23 — from `packages/tddy-core/src/log_backend.rs` by `#carve` 12/14 (PR #522), with `git mv`
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-23 | 862 | first detection, at the file's new home. The size is inherited from `tddy-core`, not grown: no line of the file changed |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | config types and their serde: `LogConfig` … `LogRotation`, `impl LogOutput`, the `Deserialize`/`Serialize` impls (L16–223) | ~210 |
| B | policy matching: `matches_selector`, `matches_target`, `matches_pattern`, `MatchedPolicy`, `resolve_logger`, `find_matching_policy`, `default_log_config` (L224–378) | ~155 |
| C | rotation: `collect_file_outputs`, `collect_file_paths`, `rotate_log_files` (L673–781) | ~110 |

Seams A and B leave about 500 lines of logger state, init and output. Add C to get clear of the budget. Prove the seams with `restructure check --deep` before applying them.
