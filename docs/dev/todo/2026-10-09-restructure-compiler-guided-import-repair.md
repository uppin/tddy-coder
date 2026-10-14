# 2026-10-09 — `restructure apply` has no compiler-guided import repair

**Category:** Future enhancement (deferred design)
**Source:** #reshape 4/19 (`extract-method-clean`), developer decision F5 (2026-10-09); candidate first written in
[the lifecycle-destructure apply gaps](./2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md)
§ Design candidates

When an apply's compile gate fails on a name the moved or extracted code lost, the engine stops and leaves the fix to a
person. The candidate: after the gate fails, read rustc's own "cannot find … in this scope" / "consider importing" /
"trait … is implemented but not in scope" suggestions in the files the run wrote, apply only those that restore a binding
the **original** files declared (rebased for the new module's depth), re-check, and undo a round that does not reduce the
errors. One mechanism with the compiler as the oracle could cover the remaining shapes: an attribute or derive macro bound
by a parent `use` (gap G, if a shape survives the lexical carry), test children that reach a name through `use super::*`
(gap M), and a named trait import used only for its methods
([its own entry](./2026-10-09-restructure-named-trait-import-used-only-for-methods.md)).

## Why deferred

#reshape 4 closed K and I with two narrow fixes that `check --deep` also sees (respelling an unimported signature type as
the origin spells it; carrying the parent's `use … as _;`). The repair needs what the run does not keep today:

- **The files' pre-run text.** The journal stores content hashes (`journal.rs`, `JournalRecord::pre`/`post`); only a
  transactional group keeps pre-images.
- **Suggestions rustc marks `MaybeIncorrect`.** The tidy's parser keeps `MachineApplicable` fixes only
  (`runner/tidy/diagnostics.rs`, `fix_of`), so a candidate filter keyed on the original bindings is new code.
- **Apply-only.** It would run after the gate, so `check --deep` would still call a plan clean that apply then repairs.
- It sits in `runner/compile_gate.rs`, which #reshape 10 changes (destination packages in the gate).

## What would make it worth doing

A real plan that fails the gate on an import none of the lexical carries reach. Record the plan and the rustc error here
first.
