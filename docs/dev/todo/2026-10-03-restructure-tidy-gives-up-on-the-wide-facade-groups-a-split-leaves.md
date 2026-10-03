# 2026-10-03 — the apply tidy gives up on the wide facade groups a split leaves, and fails the run

**Category:** Engine defect (`restructure apply` fails after a correct split)
**Source:** `#live-plan` 7/15, [#539](https://github.com/uppin/tddy-coder/pull/539) — splitting
`backends/rust.rs` into nine modules with one nine-operation `extract_module` plan (4,488 → 2,595
production lines). The plan is `plan-rust1`; the failing tree is not committed.

## What happened

Every operation applied (`[9/9] op 8: ExtractModule -> 2 file(s) applied`), the compile gate passed
(`paths: 3 path(s) re-rooted for the new module`), and then the **tidy** failed the run:

```text
the tidy was undone: this round's import edits are restored, because neither removing nor gating for
tests (placeholder_sites) leaves a tree that compiles
packages/tddy-code-restructuring/src/backends/rust.rs:2961: error[E0425]: cannot find function `placeholder_sites` in this scope
packages/tddy-code-restructuring/src/backends/rust.rs:2966: error[E0425]: cannot find function `placeholder_sites` in this scope
```

The tree left on disk compiles (only `unused_imports` warnings); the **run** is reported failed, so
the nine modules are not committed and the plan has to be rolled back and re-run.

## The shape that defeated it

Nine `named` facades, one per new module, each a single-line group in the parent:

```rust
mod placeholder_checks;
use placeholder_checks::{refuse_inferred_placeholder, carries_placeholder_type,
    refuse_residual_placeholder, Block, declares, placeholder_sites, is_identifier_byte};
```

rustc reports every such group **once per compile unit**, with different name sets (13 findings over
11 groups for this plan; `unused-imports.txt` of the failing run). For the group above the library
unit calls five names unused, the test unit only `is_identifier_byte`. So four names in one group are
used by the parent's `#[cfg(test)]` module through `use super::*`, and one is used by nothing.

## Root cause

**Established from the code and the output:**

1. **The engine's own `named` facade over-lists.** It re-exports every item "reached from outside the
   new module", and the parent's own code and tests count as outside. The result is 11 wide groups
   whose members are mostly unused in the library build. The tidy exists to clean exactly this, so it
   is handed its hardest input by design.
2. **The tidy repairs a round once.** `tidy.rs::repair` redoes a broken round with the errors' names
   gated for tests; if that redone round still fails, `round.placement.is_some()` makes the next
   `repair` return `Failed(undone(..))` immediately — "a round that was already redone is undone for
   good". There is no second iteration.
3. **The failing check is the second one.** The error lines are the redone round's, not the first
   round's: `placeholder_sites` is quoted again after it was supposedly gated, and its line numbers
   (2961, 2966) differ from the tree's (2920, 2925) because the round had already rewritten the file.
   So after gating, `placeholder_sites` still did not resolve for the tests.
4. **Why it was not caught:** the tidy's tests (`gates_one_member_of_a_group_and_removes_the_other`,
   `gates_the_member_only_tests_use_…`) cover one gated member beside one removed member in one group.
   Nothing covers several gated members in a group, a group where the library's unused set strictly
   contains the test unit's, or eleven groups in one file in one round.

**Not yet established (hypothesis, to be reproduced):** *why* the gated `placeholder_sites` did not
resolve. The likely mechanism is that the group statement is rewritten by `gating::place` (kept
members + one `#[cfg(test)] use …` item per gated member) while the lib unit's and the test unit's
member-level removal spans for the **same statement** overlap it; `apply_fixes` keeps edits only while
`edit.end <= floor` and silently drops the rest, so the rewrite and a removal can cancel or corrupt
each other. A naive replay of every `MachineApplicable` span of this plan on the pre-tidy tree produces
`error: unexpected closing delimiter` — overlapping spans from the two units do not compose, which is
consistent with this. It has not been shown that `apply_fixes` is what dropped the gated item.

## Why it matters

A split that is correct (`verify`: every statement accounted for) fails its apply, and the only
recourse is a hand edit of eleven import groups — the cleanup the engine promised to do. It is also
the *first* thing a large split meets: every earlier split (seven, up to 626 lines) had facades narrow
enough to place.

## What would close it

Reproduce on the saved pre-tidy tree (`rust.rs`, the nine modules, `facade-block.txt`,
`unused-imports.txt`, `apply.out`), find which edit dropped the gated item, and then either:

- **compose the two units' fixes per statement** — when a `use` statement has fixes from both units,
  derive its final text from the *sets* of names (unused in the library unit, unused in the test unit):
  names unused in both are removed, names unused only in the library unit are gated, the rest are
  kept; never apply the compiler's member spans for such a statement; and
- **iterate the repair** while each iteration reduces the set of failing names, instead of one retry.

And, upstream of the tidy, stop the facade over-listing: a `named` facade should list only items the
*library* build of the parent reaches; items only its tests reach belong behind `#[cfg(test)]` from the
start. Tests: a group with four gated and one removed member; the lib set strictly containing the
test set; eleven groups in one file in one round.

Fixed on this PR when found; this entry is deleted at wrap with the final root cause recorded in the
change-history entry.
