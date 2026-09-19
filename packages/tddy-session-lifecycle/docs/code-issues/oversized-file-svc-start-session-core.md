# oversized-file: svc_start_session_core.rs

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_start_session_core.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **908 production lines** · budget 500 · the file is **one function**
**Thresholds breached:** length 908 > 500
**Restructure:** two-plan `extract_method --variant module` → `extract_module_to_file` — designed, not applied
**Status:** Open — **unclaimed**
**Related:** `complexity-svc-start-session-core-start-session-core.md` — the same code measured as a function

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 908 | 896 → 908 in this PR (the new `SandboxedCodebase` placement arm) |

## What the tool found

uses + one `impl` + one method, `start_session_core`, body L54–907 — **853 lines**. There is
genuinely nothing else to move out, which the companion complexity record already states.

## What would close it — designed seam

`extract_module` has no selection of items to group, so the only route is the two-plan recipe:
`extract_method --variant module` over a body range, then a **separate** plan whose
`extract_module_to_file` anchors a caret on the resulting `mod` keyword.

Candidate ranges — each a self-contained `if` that early-returns:

| Range | What | Lines |
|---|---|---|
| L274–424 | `if req.session_type.trim() == "workspace"` | 151 |
| L427–528 | `if req.session_type.trim() == "claude-cli"` | 102 |
| L531–630 | `if req.session_type.trim() == "cursor-cli"` | 100 |
| L659–772 | the `*_for_spawn` binding run | ~114 |

Cutting the three session-type branches: ~353 out → parent **~555**, and the function 853 → ~500.
A fourth cut is needed to clear either gate.

## Constraints a later session must know

- **`extract_method` alone removes zero file lines** — the result lands in the same file. Only the
  second plan's `extract_module_to_file` moves it out.
- `to_file: true` **cannot** collapse this pair: the `mod` keyword the second step aims at is text no
  original coordinate maps to, so the ledger reports the anchor as inside removed text. Plan 2 must
  clear `.restructure/`, re-hash and re-anchor against the tree plan 1 left.
