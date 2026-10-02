# 2026-10-02 — Split `runner/entry_points.rs` (814 production lines, budget 500)

**Category:** Deferred (consented)
**Source:** `/pr-wrap` file-length gate on #538 (`#live-plan` 2/7)

`packages/tddy-code-restructuring/src/runner/entry_points.rs` is 814 production lines. #538 grew it and the file was
already over budget. The record is `packages/tddy-code-restructuring/docs/code-issues/oversized-file-runner-entry-points.md` (open,
unclaimed).

**Why deferred.** #539 (`live-plans`) and #540 (`move-paths`) are stacked on this PR and continue to
edit the surrounding code; a split inside #538 would rewrite paths under both. The developer
consented to deferring on 2026-10-02: "DEFER WITH CONSENT ... do NOT decompose".

**What would close it.** After the `#live-plan` stack lands, extract `runner/anchor_entry_points.rs`, the shared trace plumbing, and the store-backed entry points (see the record), via `/code-restructuring`
(engine moves only, no hand edits), then re-measure; the code-issue record is deleted with the final
number recorded in the change-history entry.
