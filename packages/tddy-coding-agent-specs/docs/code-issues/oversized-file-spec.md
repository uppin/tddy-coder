# oversized-file: spec.rs

**Location:** `packages/tddy-coding-agent-specs/src/spec.rs`
**Category:** oversized-file
**Detected:** 2026-11-21 — `/pr-wrap` step 3.5 file-length gate on #979 (`#orchestra` 1/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **500 production lines** of 720 total (220 test lines) · budget 500 · **at the budget**
**Thresholds breached:** length 500 ≥ 500
**Restructure:** not designed
**Status:** Open — **unclaimed**. Split deferred under the stack-branch rule: every later `#orchestra` node (n2–n9) also edits this file
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seam table is a first reading of the item list

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-21 | 461 → 500 | first detection. #979 (`#orchestra` 1/9) added the `coding_agent.home_dirs` slot: the `CodingAgentSection.home_dirs` field, `CodingAgentSpec::home_dirs` and the load-time `validate_home_dirs` check (+39). Not split: stack-shared with n2–n9 |

## What would close it — candidate seams (not proven)

The file holds the extension owner and section parsing (L38–220), the `CodingAgentSection` schema (L235–337), the slot accessors (L338–410) and the variant model (L411–500).

| Seam | Items | Out |
|---|---|---|
| A | variants: `AgentVariant`, `VariantError` and their `impl CodingAgentSpec` block (L411–500) → `spec/variants.rs` | ~90 |
| B | section validation: `validate_home_dirs` and the per-slot checks in `parse_section` → `spec/validate.rs` | ~40 |

Seam A alone brings the file to about 410 lines. Prove it with `restructure check --deep` once the `#orchestra` stack has landed.
