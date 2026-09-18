# 2026-09-18 — `extract_module` cannot see sibling seams cut by the same plan

**Category:** Defect — check/apply parity
**Source:** `#carve` node 3, [#490](https://github.com/uppin/tddy-coder/pull/490) Phase A — one plan
cutting **eight** `extract_module --to_file` seams out of `crate_move.rs` (1,998 lines)

`restructure check --deep` reported **`no findings`** and `apply` reported **8 of 8 applied**. The
tree then **did not compile**: 13 errors, needing 6 hand fixes before `cargo build` passed. Every fix
was import- or visibility-only — no moved code had to be written — so the seams themselves were
right and the operation's bookkeeping around them was not.

`--deep` is documented as the gate that "resolves each operation through the same path `apply` uses",
and it is the reason to prefer it over a plain `check`. It held for each operation **in isolation**
and still let a plan through that could not compile, which is the same class of gap
[#488](https://github.com/uppin/tddy-coder/pull/488) closed for single operations.

## One root cause

**`extract_module` reads "outside the new module" as "the parent file".** A seam cut earlier in the
same plan is no longer in the parent, so references living in it are invisible to all three passes at
once — the reference survey, the import pass, and the visibility restoration. The defect therefore
only appears in a **multi-seam** plan, and gets likelier the more seams a plan cuts.

| Pass | What it got wrong | Observed |
|---|---|---|
| reference survey | rewrote a reference as `<modname>::<Item>`, which resolves from the parent and **not** from a sibling submodule | `destination::Destination` in `refusals.rs` and `cluster.rs`; `module_home::defining_crate` in `header.rs` — `E0433` |
| import pass | left helpers unqualified in a module extracted **after** the one defining them | `replacement`, `relative_from` in `moving.rs` — `E0425` ×5 |
| visibility restoration | restored an item to private after deciding nothing outside its new module reached it, while a **sibling** seam still did | `crate_holding`, `WrittenPath`, `PlannedRewrite` — `E0432`, private-type and private-in-public errors |

The visibility row is the most misleading of the three: the operation *reports* every widening it
performs, so a widening it wrongly declined to perform is reported nowhere and shows up only as a
compile error in a file the operation says it finished with.

## Why this matters beyond the one file

A `--deep` check that passes is what a planner is told to trust before paying for an apply. Here it
bought nothing for the failure mode that actually occurred, and the cost lands *after* the apply has
rewritten nine files — the point at which backing out is most expensive.

Every remaining `#carve` node splits an oversized file before moving it, which is a multi-seam plan
by construction.

## Possible fixes, not chosen here

- Have the three passes survey against **the plan's projected module tree** rather than the parent
  file as it stands — the operation already knows which seams precede it.
- Failing that, make `check --deep` type-check the **projected** tree once after resolving all
  operations, so a plan that cannot compile is refused before any write.
- Cheapest stopgap: when a plan cuts more than one seam from one file, qualify rewritten references
  as `super::<modname>::<Item>` and skip visibility narrowing entirely — over-widening is recoverable,
  a non-compiling tree is not.

## Not fixed by `#carve` 3/9

That node's `## Boundaries` reserve `check`/`apply` parity itself to other nodes; it consumed the
parity work as a dependency rather than extending it. The six hand fixes are in its Phase A commit
and are import/visibility only.
