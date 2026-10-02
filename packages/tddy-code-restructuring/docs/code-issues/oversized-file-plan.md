# oversized-file: plan.rs — the plan vocabulary, the item-path types and the header codec in one module

**Location:** `packages/tddy-code-restructuring/src/plan.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #537
**Metrics:** **799 production lines** (2026-10-02, #537) · budget 500 · measured before the first `#[cfg(test)]`
**Restructure:** required — `extract_module --to_file` × 2 (`/code-restructuring` territory)
**Status:** Open — split deferred; later `#live-plan` nodes touch this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 433 → 799 | #537 (`#live-plan` 1/7): +366. Under budget at the merge base; the item-anchor contract (`Anchor::Item`/`Items`, `ItemPath`, `ItemSegment`, `Fingerprint`) and the v2 snapshot header (`FileHint`, `HintedHeader`, `rfc3339`) landed here rather than beside it |

Reproduce, from the repo root:

```bash
f=packages/tddy-code-restructuring/src/plan.rs
awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' "$f"                                  # now
git show "$(git merge-base origin/master HEAD):$f" | awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}'   # before
```

## What the gate found

799 production lines, 1.6× the budget; the file crossed it in this PR. Three concerns share it.

## Why it was not split in #537

The later `#live-plan` nodes (2/7 onward) extend the anchor and header types in this same file.
Splitting it inside #537 would rewrite paths under every PR stacked on it. The developer chose to
defer on 2026-10-02.

## What would close it

Two `extract_module` moves, after the `#live-plan` stack lands:

- `plan/item_path.rs` — `ItemPath`, `ItemSegment`, `split_path`, `read_segment`, the `TryFrom`,
  `From` and `Display` impls, and `Fingerprint`
- `plan/header.rs` — `FileHint`, `SnapshotHeader`, `HintedHeader`, the schema-version constants,
  `hint_of`, `rfc3339`, `header_version`

That leaves `Anchor`, `RefactorKind`, `RefactorOp` and `Plan` with `parse_op` — about 400 lines.

## Verified by hand

2026-10-02: counted with the command above at the merge base (433) and the working tree (799).
Inline test modules begin after the counted region; nothing counted is test code.
