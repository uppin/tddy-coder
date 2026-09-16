# 2026-09-16 — The complexity cache is unbounded, and the daemon has no reaping for analysis state

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset, M7

`InMemoryComplexityCache` (`packages/tddy-code-analysis/src/complexity_cache.rs`) keys each scored
function set by a hash of the **content** it scored. That is what makes it correct — an unchanged
file is not rescored, a changed one always is, and two files with identical contents are scored once.
It is also why it grows without limit: over a tree under active edit it accumulates one entry per
*version* of a file, not per file.

`tddy-index-daemon` holds it process-wide (`src/index.rs`), and the daemon has an idle-reaping story
for **language servers** only. Nothing reaps analysis state.

## What closing it would take

An eviction policy — an LRU bound, or a tie to the root's idle timer so a root going idle drops its
scores along with its language server. The second is more consistent with how everything else in the
process is bounded, but content addressing is deliberately *not* per-root (a hash is a hash across
worktrees of one repo), so the two would have to be reconciled.

Not urgent: the entries are small, and a long-lived daemon over a quiet tree is stable. It matters for
a daemon left running across a day of editing.
