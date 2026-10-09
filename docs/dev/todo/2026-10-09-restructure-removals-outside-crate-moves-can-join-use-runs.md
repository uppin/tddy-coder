# 2026-10-09 — `reparent_module` and `move_item` removals can join two `use` runs that rustfmt then re-sorts

**Category:** Restructure engine defect (reviewability of the diff, not correctness) — unreproduced
**Source:** #reshape 3/19 (`feature/reshape/tidy-facades`)

#reshape 3 keeps a crate move's removed `mod` declaration from joining the `use` runs above and below it
(`crate_move/manifest_edits.rs` `separates_use_runs`): rustfmt sorts within a run, so a joined run re-sorts lines
the plan never wrote. The same can happen wherever an operation removes the only line between two `use` runs:
`reparent_module` removing `mod x;` from the old parent (`backends/rust/module_reparent/declaration.rs` `Declaration.lines`)
and `move_item` removing a moved item that sat between two runs (`backends/rust/item_move/`).

**What would close it:** apply the same rule (a blank line stays when the removed lines separated two runs) in
those removers, with a fixture per operation.

**Why deferred:** neither has been seen doing it in a real run, and both are other nodes' code this stack does not
otherwise touch; the rule and its helper exist after #reshape 3, so the fix is small once a reproduction exists.
