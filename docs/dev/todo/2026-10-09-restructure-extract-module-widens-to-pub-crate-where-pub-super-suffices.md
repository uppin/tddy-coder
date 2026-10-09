# 2026-10-09 — `extract_module` widens to `pub(crate)` where `pub(super)` is the reach needed

**Category:** Defect — visibility width
**Source:** `#reshape` 2/19 planning; split out of item X (second half) of
`2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md`, which no `#reshape` node claims

When a moved private item is still reached from outside its new module, `restore_visibility` keeps
the assist's `pub(crate)` (`backends/rust/visibility.rs:15` `WIDENED`, `:82-96`). A private item was
reachable from its module and that module's descendants; after `extract_module` the same reach is
`pub(super)`. `pub(crate)` exceeds the private types in the item's signature, which is
`private_interfaces` under `-D warnings`. In every `#524` apply (plans 12, 13, 17, 21) the hand fix
was `pub(super)`.

## Possible fix

Widen a previously private item to `pub(super)` when every reference the survey found sits in the
original parent module or below it (by module path of the referring file), and to `pub(crate)` only
otherwise; report the width chosen.

## Why deferred

Not claimed by any `#reshape` node, and `#reshape` 2 deliberately kept the width unchanged so its
parity tests pin one behaviour. It needs module-path arithmetic across referring files, which the
visibility pass does not do today.
