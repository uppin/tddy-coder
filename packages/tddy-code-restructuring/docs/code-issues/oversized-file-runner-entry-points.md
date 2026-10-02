# oversized-file: runner/entry_points.rs — every `restructure` entry point and their shared plumbing

**Location:** `packages/tddy-code-restructuring/src/runner/entry_points.rs`
**Category:** oversized-file
**Detected:** 2026-10-02 by the `/pr-wrap` file-length gate on #537
**Metrics:** **602 production lines** (2026-10-02, #537) · budget 500 · measured before the first `#[cfg(test)]`
**Restructure:** required — `extract_module --to_file` × 2 (`/code-restructuring` territory)
**Status:** Open — split deferred; later `#live-plan` nodes touch this file

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-02 | 506 → 602 | #537 (`#live-plan` 1/7): +96 net. Added `item_anchors`, `open_run_resolving_anchors`, `resolve_item_anchors`, `refuse_a_continued_item_plan` and `unresolvable_without_a_server`; removed the range-only `anchors` (−35). Barely over budget at the merge base, now 1.2× |

Reproduce, from the repo root:

```bash
f=packages/tddy-code-restructuring/src/runner/entry_points.rs
awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}' "$f"                                  # now
git show "$(git merge-base origin/master HEAD):$f" | awk '/#\[cfg\(test\)\]/{exit} {n++} END{print n}'   # before
```

## What the gate found

602 production lines. The file was already at 506 at the merge base; #537 added the item-anchor
entry points to it.

## Why it was not split in #537

Later `#live-plan` nodes add entry points and change the anchor-resolving run setup here, so a split
now would rewrite paths under every PR stacked on #537. The developer chose to defer on 2026-10-02.

## What would close it

- `runner/anchor_entry_points.rs` — `item_anchors`, `open_run_resolving_anchors`,
  `resolve_item_anchors`, `refuse_a_continued_item_plan`, `unresolvable_without_a_server`
- `runner/trace.rs` (or fold into `options.rs`) — `TRACE_VARIABLE`, `wants_trace`, `registry_for`,
  `registry_for_static`, `report_visibility`, `progress_line`: the plumbing every entry point shares

`run`, `dispatch`, `apply`, `snapshot`, `status` and `check` stay; roughly 400 lines.

## Verified by hand

2026-10-02: counted with the command above at the merge base (506) and the working tree (602).
