# 2026-10-03 — Check/apply parity for cross-crate moves

**Type:** Fix

`#live-plan` 6/7, PR [#543](https://github.com/uppin/tddy-coder/pull/543). Product entry:
[2026-10-03-check-parity.md](../../../../docs/ft/coder/changelog/2026-10-03-check-parity.md).
Single-package change, so no cross-package entry.

`move_preconditions` reports two more cases that `apply` cannot build, so `check` and `check --deep`
no longer pass them.

- **Body path** (`crate_move/preconditions.rs`, `stays_behind_through_a_body`): reads the move's path
  survey for a path that is in a body, outside `#[cfg(test)]`, defined in the origin and inside a module
  that stays behind. The remedy never suggests a cluster. Plan-aware: `moved_by_earlier_operations`
  counts modules earlier operations moved to the same destination as gone.
- **Merge** (`destination_already_has_the_module`): a destination root that declares the module, or a
  file at the target path.
- **`member_op`** keeps every anchor of a cluster in `also` when a member is checked, so a member's body
  path to the module anchoring the cluster is read as travelling with it; `with_anchor` dropped it.
- `header::travels_with` is `pub(crate)`; `survey.rs` is unchanged.

Resolved backlog entries: *`check` misses a body path to a module staying behind* and *`check` passes a
move whose module name the destination already has* (both 2026-09-25).

## Final measurements

| Measure | Result |
|---|---|
| `tddy-code-restructuring` tests | 702 passed, 0 failed, 1 ignored (scoped run) |
| `crate_move/cluster.rs` production lines | 624 → 625 (already over the 500 budget; split deferred, two other nodes of the stack edit it) |
| `crate_move/preconditions.rs` production lines | 84 → 268 |
| Longest new function | `stays_behind_through_a_body`, 36 lines |

## Left open

- The stranded-sibling finding reads the top-level `use` header only, and a `use` nested in a function
  is read by neither check; the first is a backlog entry
  (*the stranded-sibling finding reads only the use header*, 2026-10-03).
- `stays_behind_through_a_body` guards `defining_crate != origin` ahead of a `strip_prefix` that skips the
  same paths today; it is kept so the invariant does not rest on a string prefix of `defined_at`.
