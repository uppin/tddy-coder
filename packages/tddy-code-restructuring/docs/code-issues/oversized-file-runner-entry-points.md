# oversized-file: runner/entry_points.rs — every `restructure` entry point and their shared plumbing

**Location:** `packages/tddy-code-restructuring/src/runner/entry_points.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #537
**Metrics:** **814 production lines** (2026-10-02, #538) · budget 500 · measured before the first `#[cfg(test)]`
**Restructure:** required — `extract_module --to_file` × 2 (`/code-restructuring` territory)
**Status:** Open — regressed 2026-10-02 (#538 grew it 602 → 814); split deferred with consent; later `#live-plan` nodes touch this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 506 → 602 | #537 (`#live-plan` 1/7): +96 net. Added `item_anchors`, `open_run_resolving_anchors`, `resolve_item_anchors`, `refuse_a_continued_item_plan` and `unresolvable_without_a_server`; removed the range-only `anchors` (−35). Barely over budget at the merge base, now 1.2× |
| 2026-10-02 | 602 → 814 | #538 (`#live-plan` 2/7): +212. The store-backed `apply_held_plan` loop, `load`/`unload`/`plans` entry points and resume wiring landed here. 1.6× budget. The developer consented to deferring the split on 2026-10-02 |

Reproduce, from the repo root:

```bash
f=packages/tddy-code-restructuring/src/runner/entry_points.rs
awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' "$f"                                  # now
git show "$(git merge-base origin/master HEAD):$f" | awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}'   # before
```

## What the gate found

814 production lines (1.6× the budget). The file was at 506 before #537, 602 after it; #538 added the plan-store entry points and the apply loop's store plumbing.

## Why it was not split in #537

Later `#live-plan` nodes add entry points and change the anchor-resolving run setup here, so a split
now would rewrite paths under every PR stacked on #537. The developer chose to defer on 2026-10-02.

## Why it was not split in #538

#539 (`live-plans`) and #540 keep changing the entry points and the apply loop here, so a split inside #538 would rewrite paths under every PR stacked on it. The developer consented to the deferral on 2026-10-02. Open and unclaimed; follow-up: `docs/dev/todo/2026-10-02-split-oversized-runner-entry-points-rs.md`.

## What would close it

- `runner/anchor_entry_points.rs` — `item_anchors`, `open_run_resolving_anchors`,
  `resolve_item_anchors`, `refuse_a_continued_item_plan`, `unresolvable_without_a_server`
- `runner/trace.rs` (or fold into `options.rs`) — `TRACE_VARIABLE`, `wants_trace`, `registry_for`,
  `registry_for_static`, `report_visibility`, `progress_line`: the plumbing every entry point shares

The store-backed entry points (`load`, `unload`, `plans`, `apply_held_plan` and its helpers) are a third candidate module, `runner/store_entry_points.rs`.

`run`, `dispatch`, `apply`, `snapshot`, `status` and `check` stay; the remainder should land near the budget once all three moves are made (re-measure; it was 814 before any).

## Verified by hand

2026-10-02: counted with the command above at the merge base (506) and the working tree (602).
