# oversized-file: store.rs

**Location:** `packages/tddy-model-registry/src/store.rs`
**Category:** oversized-file
**Detected:** 2026-09-22 — `/pr-wrap` step 3.5 file-length gate (#493, which edited one doc-comment line only)
**Metrics:** **1,139 production lines** (no `#[cfg(test)]` module; tests live in `tests/`) · budget 500 · **2.3× over** · `impl ModelRegistryStore` alone is **~668 lines** (L98–765)
**Thresholds breached:** length 1139 > 500
**Restructure:** not designed
**Status:** Open — **unclaimed**
**Verified:** ⚠ **not hand-verified** — line counts are machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-22 | 1139 | 1139 → 1139 in #493 — a one-line doc-comment path fix; not grown |

## What would close it — candidate seams (not proven)

| Seam | Items | Out |
|---|---|---|
| A | schema and migration: `ensure_schema`, `add_column_if_missing` (L1021–1125) | ~105 |
| B | file permissions: both `precreate_owner_only` and `restrict_to_owner` variants, `io_failure` (L951–1020) | ~70 |
| C | validation: `reject_an_oversized_system_prompt`, `validate_base_url`, `validate_tools`, `kind_slug` (L766–940, minus the query helpers) | ~100 |
| D | `impl ModelRegistryStore` split by entity (providers / models / assistants) | ~668 in total |

Seams A–C together leave the file at around 860 lines, so seam D is needed to get under budget.
Prove the seams with `restructure check --deep` before applying them.
