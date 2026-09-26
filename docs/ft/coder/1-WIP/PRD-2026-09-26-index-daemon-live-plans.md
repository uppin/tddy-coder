# Index daemon live plans — every loaded plan stays current as the tree moves - PRD

**Date**: 2026-09-26
**PRD Type**: Enhancement

## Affected Features

- **Primary Feature**: [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md) — loaded
  plans are refreshed after every applied operation and every external tree change; stale operations
  are reported and refused.
- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `restructure
  snapshot` re-resolves a plan's anchors against the tree; the § Known limitations entry about
  stale plans across a multi-plan carve.
- **Related surface**: `.agents/skills/code-restructuring/` — the multi-plan workflow.

## Summary

A carve is several plans applied in sequence under one root. Applying the first moves the lines,
files and items the others anchor into, and a PR landing underneath moves them again. With the plan
store, the plan being applied stays current; every *other* plan still goes stale.

This PRD makes **every plan the daemon holds for a root** current: after any applied operation, from
any loaded plan, and after any change to the tree the daemon did not make itself, each loaded plan's
anchors are refreshed and flushed — `file` hints follow moved items, position hints and per-file
hash + update time are rewritten, and an operation whose anchored item itself changed is marked
**stale**, reported by `Check`, `PlanStatus` and `ListPlans`, and refused by `Apply` by id.

## Background

- [`2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md`](../../dev/todo/2026-09-24-restructure-snapshot-cannot-rebase-a-stale-plan.md):
  six of #524's plans went stale when #508 edited their files; a throwaway difflib script
  re-anchored them and one endpoint had to be fixed by hand.
- `packages/tddy-index-daemon/src/tree_changes.rs` already computes which files changed between
  two snapshots of the tree, to tell rust-analyzer via `workspace/didChangeWatchedFiles`.

## Proposed Changes

### What's Changing

1. **Cross-plan refresh.** An operation applied from plan P folds its workspace edit and renames into
   every other plan loaded for the same root: their hints, relative ranges and `file` hints are
   updated the way P's own pending ops are, and their fingerprints recomputed for items the edit
   touched *only where the edit lay outside the anchored range*. An edit that overlapped another
   plan's anchored range marks that op stale.
2. **External change refresh.** When the tree changes underneath the daemon (a checkout, a rebase,
   a hand edit), each loaded plan's anchors in the changed files are re-resolved through the item
   path. Found and fingerprint-equal → hints updated. Found with a different fingerprint, or not
   found → the op is **stale**.
3. **Stale ops** carry the reason (`item changed`, `item not found in <file>`, `edited by <plan>#<op
   id>`). `ListPlans`, `PlanStatus` and `Check` report them; `Apply` refuses a run whose next op is
   stale, naming the op id and the reason, before any write.
4. **Per-file hints** (`files.<path>.sha256`, `.modified`) are rewritten on every refresh.
5. **`restructure snapshot <plan>`** re-resolves every item anchor of the plan against the current
   tree and rewrites hints, fingerprints of *unchanged* items and the header — refusing, per op,
   where an item changed. This is the "rebase a stale plan" the backlog entry asked for, for item
   anchors; a v1 range plan is told to convert with `anchors --at`.
6. All refreshed plans flush through the plan store's existing flush path.

### What's Staying the Same

- The store, its RPCs, flush timing and the clobber refusal.
- Unloaded plans are never touched.
- A stale op is never silently re-targeted; the author re-anchors it.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: `plan_store` refresh API (fold an edit set; re-resolve a file's
  anchors); `snapshot` re-resolution.
- `tddy-index-daemon`: the apply loop notifies the root's store after each op; the tree-change path
  re-resolves loaded plans; stale state in `ListPlans` / `PlanStatus` / `Check` events.

### User Impact

A multi-plan carve no longer needs re-anchoring between plans or after a rebase. What cannot be kept
current is named, per op, before anything is written.

## Acceptance Criteria

- [ ] Plans A and B loaded; applying A's op that inserts lines above B's item updates B's flushed
      hint, and B then applies the same edit as against a fresh tree.
- [ ] A's op moving a module into another crate updates B's `file` hint for an item in that module.
- [ ] A's op editing inside B's anchored range marks B's op stale with `edited by A#<id>`, and B's
      `Apply` is refused naming it before any write.
- [ ] A hand edit above B's item (outside the daemon) refreshes B's hint within the refresh
      interval; a hand edit inside B's item marks the op stale.
- [ ] `ListPlans` and `PlanStatus` report stale ops with their reasons.
- [ ] `restructure snapshot` on a plan written before three lines were inserted above its items
      rewrites hints and header and then applies; an op whose item changed is reported and left.
- [ ] An unloaded plan's file is byte-identical after the daemon applies another plan.

## References

- [Warm code-intelligence daemon](../warm-code-intelligence-daemon.md)
- [Rust code restructuring](../rust-code-restructuring.md)
