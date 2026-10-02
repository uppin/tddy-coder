# 2026-10-02 — `ItemChanged` names the item and its file but not the operation

**Category:** Future enhancement
**Source:** `/pr-wrap` validation of #537 (`#live-plan` 1/7)

`RestructureError::ItemChanged { item, file }` (`packages/tddy-code-restructuring/src/lib.rs:106`) is
raised by `resolve_item_anchors` (`item_anchor.rs:254`) while the whole plan is resolved at run open,
before any operation runs. The PRD's first wording asked for the refusal to name the item and the
operation; the PRD and the permanent docs now say item and file.

An author with a plan of forty operations over one item sees which item changed but not which
operation line to re-anchor. The resolver has the operation index in hand when it walks a plan's
anchors; threading it into the error is small, and the daemon's `status.rs` match needs no change
since the variant keeps its class.

**Why deferred.** Not an acceptance criterion once the PRD was amended, and the refusal already names
the remedy (`restructure anchors`). Worth doing when `#live-plan`'s plan store re-resolves per
operation, since that is when an operation index is natural.
