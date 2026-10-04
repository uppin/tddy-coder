# 2026-10-04 — the same-crate move acceptance files are not `rustfmt`-clean

**Category:** Hygiene (CI gate)
**Source:** commit `61bb5fda` (plan and red tests), seen while implementing `move_item`

## What happens

`cargo fmt -p tddy-code-restructuring -- --check` reports differences in four files committed with the red
tests: `tests/move_item_acceptance.rs`, `tests/reparent_module_acceptance.rs`,
`tests/anchors_package_relative_path.rs` and `tests/same_crate/mod.rs`. Running `cargo fmt -p` rewrites them,
which is why the implementation commit does not: the tests are the contract and were left byte for byte as
approved.

## What to do

Run `cargo fmt -p tddy-code-restructuring` once, in a commit of its own that touches no assertion, before the
PR is opened (the fmt gate is red on these four files otherwise).
