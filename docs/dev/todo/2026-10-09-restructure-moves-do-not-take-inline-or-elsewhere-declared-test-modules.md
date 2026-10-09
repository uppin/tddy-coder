# 2026-10-09 — crate moves do not take inline test modules or test modules declared outside the moved module's parent

**Category:** Engine feature gap (restructure engine)
**Source:** #reshape 14/19 (`feature/reshape/tests-follow`)

## What happens

`#reshape` 14 considers only out-of-line `#[cfg(test)] mod t;` declarations in the file that declares a moved module.
Two shapes stay behind even when they test only moved code:

1. an inline `#[cfg(test)] mod tests { … }` in the parent whose tests exercise a moved sibling — it is part of the
   parent's text, so it cannot travel as a file;
2. a test module declared elsewhere, e.g. a crate-root `#[cfg(test)] mod foo_tests;` that tests the nested `a::foo`.

## What should happen

1. Offer an `extract_module` of the inline block's relevant tests first (plan-level), or split it as part of the move.
2. Widen the considered set to every gated declaration in the origin crate, classified by the same rule.

## Why deferred

Neither shape appeared in `#carve`. (1) needs a seam through a test module, which `extract_module` handles poorly today
(apply-gaps item M); (2) widens the scan from one file to the whole crate and needs a cost bound first.
