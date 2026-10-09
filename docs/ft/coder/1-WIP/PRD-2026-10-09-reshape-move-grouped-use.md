# Cross-crate moves split grouped `use` lines, see co-movers through glob facades, and `check` reads every path - PRD

**Date**: 2026-10-09
**PRD Type**: Enhancement (removes a refusal) and bug fix (check/apply parity)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `## Path survey` (how a group is
  rewritten, how a path through a glob facade and a `pub(in …)` restriction are read, what `check` reports),
  `## Rust operations (v1)` (`move_module_to_crate` / `move_cluster_to_crate` caller re-points), `### repoint_facade_imports`
  (Rule S gains nested-member flattening), `## Known limitations` (three bullets removed or narrowed).

No other feature document changes. No new operation, field, flag or wire message.

## Summary

`move_module_to_crate` and `move_cluster_to_crate` stop refusing a grouped `use` whose leaves need different
qualifiers after the move. They rewrite it the way `repoint_facade_imports` already does (Rule P in place when every leaf
agrees, Rule S otherwise: kept members stay in the group, each re-pointed member becomes its own `use`). The same rule is
applied to callers the move re-points, so a re-point is never written inside someone else's `crate::{…}` group.
A path that reaches a co-moving module through a glob facade of the origin (`crate::connection_service::SeededAgentClones`
where `pub use seed_codebase::*;`) counts as co-moving. A `pub(in crate::<origin module>)` restriction is read as a
visibility, not as a path naming the origin crate. `check`'s stranded-sibling finding reads every path `apply` reads, so
`check` no longer passes a cluster `apply` then refuses.

## Background

`#carve` 21/21 (PR #536) moved three clusters out of `tddy-session-lifecycle` (R6 agents, R8 split, R9 launch). Every one was
refused by `check --deep` before any server answered, and every refusal was worked around with developer-consented hand
edits before the engine op ran, itemised in `2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move.md`:

- **Grouped `use`:** 3 groups for R6, 1 for R8, 14 flat groups and 1 nested group for R9. `crate_move/header.rs` `one_use_per_path`
  refuses by design ("one prefix is all a group has"); `repoint_facade/group.rs` Rule S already splits the same shape for
  another operation.
- **Glob facade to a co-mover:** `header.rs` `reach` tests co-movement on the path as written, not on where the origin's glob
  re-exports lead, so `crate::connection_service::SeededAgentClones` "stays behind" although `seed_codebase` is in the same
  cluster. The body-path finding already tests the followed path.
- **`pub(in crate::connection_service)`:** the survey reads the restriction as a body path into the origin, so it counts as an
  edge back. 21 occurrences were widened to `pub` by hand for R9. If the move is not refused, the engine writes
  `pub(in tddy_session_lifecycle::connection_service)`, which never compiles.

Two older entries are about the same code: `apply` refuses on every path but `check`'s stranded-sibling finding reads only the
top-level `use` header (`TODO(check-parity-header)`), and a test pins the gap
(`check_precondition_parity.rs` `a_body_path_to_an_item_at_the_crate_root_is_no_finding`, a plan `apply` refuses). With
`reexport: none`, a caller re-point can be spliced inside a grouped `use` (`use crate::{a, tddy_daemon_livekit::x::Y, b};`).
A caller group naming two members of one cluster gets one edit per member.

## Proposed Changes

### What's Changing

- **Moved file, grouped `use`.** The leaves of one `use` tree are rewritten by one shared group rule:
  - **Rule P**: if every leaf agrees on the new prefix, the prefix is replaced in place. This is today's behaviour, byte for byte.
  - **Rule S**: otherwise the members that keep their path stay in the group under the old prefix, first, and each re-pointed
    member follows as its own `<visibility> use <path>;` in member order, with the statement's indentation. A group with no
    kept member becomes only its lifted statements. A leaf whose last segment changes keeps the name it bound (`as <old>`),
    and an alias is kept.
  - **Nested members**: a nested member (`a::{x, y}`) is lifted whole when its leaves share one prefix, and flattened into
    one statement per leaf when they do not. This replaces `nested_member_reaches_two_crates` in
    both this operation and `repoint_facade_imports`; the refusal's second half is pinned by
    `repoint_facade_imports_acceptance.rs:421`, which is rewritten to the flattened output (a third pinned test; consent
    asked in the changeset, F6).
  - R6 example: `use crate::connection_service::{seed_codebase, seeded_clone_guard, SeededAgentClones};` arrives as
    `use crate::seed_codebase;` / `use crate::seeded_clone_guard;` / `use crate::seed_codebase::SeededAgentClones;`, which
    are exactly the hand edits.
- **Callers (`reexport: none`).** Re-points that fall inside a grouped `use` are rewritten by the same rule on the caller's
  statement, not spliced into the group. Caller re-points are collected across every member of a cluster before statements are
  written, so one statement gets one edit.
- **Glob facade to a co-mover.** A path counts as co-moving when either the written path or the path after following the origin's
  re-exports reaches a member. It lands under the member's last segment plus the rest of the followed path
  (`crate::seed_codebase::SeededAgentClones`). This is the same reading `stays_behind_through_a_body` already uses.
- **`pub(in …)` restrictions.** The survey marks a path written inside `pub(in …)` as a visibility restriction. A restriction
  is never an edge and adds no manifest line.
  - If it names the moved module or a co-moving module, it is rewritten to `crate::<landing>…`, as today.
  - If it names a module that stays in the origin, it becomes `pub(crate)`: the narrowest spelling that is valid in the
    destination. This node owns every respelling of `pub(in …)` in a crate move (developer decision, 2026-10-09). Widening a declaration to `pub` because code outside the moved files reaches it is cross-crate widening (`#reshape` 7, branch `feature/reshape/move-widen`), which never respells `pub(in …)`; this node does not depend on it.
- **`check` parity.** The stranded-sibling finding reads every path outside `#[cfg(test)]` that `apply`'s cycle refusal reads:
  the top-level header, nested `use` items and body paths, with restrictions excluded. `header_origin_paths` and the
  `TODO(check-parity-header)` marker are deleted.
  - When a reached path is an item of the origin's crate root, the finding's remedy no longer suggests
    `move_cluster_to_crate` (a root item cannot join `also`). It says to cut that dependency.
- **Refusals that remain**, unchanged in wording except as noted:
  - a path spelled across whitespace or comments;
  - a group that must be split but carries an attribute or doc comment. `repoint_facade_imports`' refusal is reused, so the
    attribute is never repeated;
  - a `use` statement the rule cannot read.

### What's Staying the Same

- Rule P output, and the output for every group whose leaves agree, are byte-identical to today. A move that never met the refusal
  writes the same text.
- `reexport: glob` moves still re-point no caller. The facade lines, manifests, renames and destination `pub mod` lines are unchanged.
- The dependency-cycle refusal and its wording. A split group whose kept member names something staying in the origin is still
  refused, now by that refusal instead of "write one `use` per path".
- `move_test_binary_to_crate`'s refusal of `origin::{…}` / `origin::*` in a test binary (a separate, text-only reader; deferred).
- A path through a *third* crate's re-export is not re-pointed to its defining crate (deferred; policy change).
- `repoint_facade_imports` behaviour, except that a nested member whose leaves disagree is now flattened instead of refused.
- Same-crate moves (`move_item`, `reparent_module`) and their own grouped-`use` refusals.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only.
  - The text-only group rule (`split_or_reprefix`, `Member`, `members_of`, `split_use` and the statement-span finder) moves
    **down** from `backends/rust/repoint_facade/group.rs` and `item_move` into `crate_move`. `repoint_facade` then imports it
    from there. Calling up from `crate_move` into `backends::rust` would add a `crate_move → backends` edge and a new module
    cycle, which the later crate split (stack 2) must not inherit.
  - `header.rs` `rewrite_of` writes statement-level edits through it.
  - `reach` also tests the followed path.
  - The survey's sightings gain a visibility-restriction flag.
  - `stranded.rs` reads `origin_paths`.
  - `resolve_cluster` collects caller re-points across members.
- `resolve_cluster` and `stranded_siblings` are already on node 16's function-size list and must not grow. The new logic goes
  in helpers.
- No new test binary needs a rust-analyzer. The library-level suite drives `resolve_cluster` over a fake reference set. One live
  test joins the existing, already-registered `cluster_move_acceptance` binary.

### User Impact

- A cluster move of lifecycle-shaped code needs no pre-move import surgery: the R6/R8/R9 hand-edit classes (grouped `use`,
  facade re-spelling, `pub(in …)` widening) become engine output.
- `check` may now report findings on plans it used to pass. Each is a plan `apply` already refused, so no plan that applied
  before stops applying.
- No breaking change to the plan format or the CLI.

## Implementation Plan

1. Move the group rule into `crate_move` (engine-driven `move_item` where the engine can; any hand fix after the move gets a TODO
   entry). `repoint_facade` stays green on its existing tests.
2. Nested-member flattening in the shared rule.
3. `header.rs`: statement-level rewrite through the shared rule. Delete `one_use_per_path`.
4. `reach`: co-movement on the followed path.
5. Survey: the `pub(in …)` restriction flag, and its rewrite.
6. Callers: rewrite through the shared rule, collected across the cluster.
7. `stranded.rs`: read `origin_paths`, root-item remedy wording, delete `header_origin_paths` and the TODO marker.
8. One live end-to-end test of the R6 shape (`check --deep` clean, `apply`, `cargo check`).
9. Docs at wrap.

## Acceptance Criteria

- [ ] A moved file's grouped `use` whose leaves need different qualifiers is split by Rule S (kept members first, one `use` per
  lifted member, names and aliases kept, indentation kept). A group whose leaves agree is rewritten in place, byte-identical
  to today. ([Rust code restructuring](../rust-code-restructuring.md) § Path survey)
- [ ] A nested member whose leaves need two prefixes is flattened into one `use` per leaf, in both cross-crate moves and
  `repoint_facade_imports`.
- [ ] A group that must be split under an attribute or doc comment is refused with the file, line and fix, and nothing is written.
- [ ] A path reaching a co-moving member through an in-crate glob facade lands at `crate::<member>::…` and is no edge back. The R6
  cluster (3 groups, `SeededAgentClones` through `pub use seed_codebase::*`) passes `check --deep`, applies, and the workspace
  passes `cargo check`.
- [ ] `pub(in crate::<origin module>)` in a moved file is no edge and adds no manifest line. It becomes `pub(crate)` when the
  module stays, and `pub(in crate::<landing>)` when it is co-moving. An extern path is never written into a restriction.
- [ ] With `reexport: none`, a caller's re-point inside a grouped `use` produces a group that resolves (Rule P or S), never
  `crate::{…, dest::…}`. A caller group naming two cluster members receives one statement edit.
- [ ] `check` reports a nested `use` and a crate-root body path to the origin as `apply` refuses them. A root item's remedy does
  not suggest `move_cluster_to_crate`. `header_origin_paths` and `TODO(check-parity-header)` are gone.
- [ ] `move_paths_acceptance` `a_use_group_whose_members_land_in_different_crates_…` and `check_precondition_parity`
  `a_body_path_to_an_item_at_the_crate_root_is_no_finding` (consent given 2026-10-09), and `repoint_facade_imports_acceptance`
  `a_nested_group_member_is_lifted_whole_when_its_leaves_agree_and_refused_when_they_do_not` (consent pending, F6), are
  rewritten to the new behaviour, never deleted.
- [ ] Tests pass for `tddy-code-restructuring`, scoped locally (`./test -p tddy-code-restructuring`); CI covers the rest.

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-move-grouped-use.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-move-grouped-use-initial-discovery.md`
- Backlog this node claims:
  [2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left](../../../dev/todo/2026-10-08-restructure-move-cluster-refuses-a-grouped-use-the-conversion-nodes-left.md),
  [2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move](../../../dev/todo/2026-10-08-hand-split-grouped-use-lines-before-the-agents-cluster-move.md)
  (narrowed at wrap to the third-crate re-export section),
  [2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header](../../../dev/todo/2026-10-03-restructure-stranded-sibling-finding-reads-only-the-use-header.md)
- Partial: [2026-09-09-restructure-defects-from-the-first-cross-crate-move](../../../dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)
  item 4 (caller re-point spliced inside a group)
- Package docs: [path-survey.md](../../../../packages/tddy-code-restructuring/docs/path-survey.md),
  [repoint-facade.md](../../../../packages/tddy-code-restructuring/docs/repoint-facade.md)
