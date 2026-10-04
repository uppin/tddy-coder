# 2026-10-04 — the approved `warm` test in `index_daemon_client_acceptance.rs` is not rustfmt clean

**Category:** Known failing check (`cargo fmt --check`)
**Source:** changeset
[`2026-10-04-restructure-same-crate-moves`](../1-WIP/2026-10-04-restructure-same-crate-moves.md), E3

`cargo fmt -p tddy-tools --check` reports one diff in `packages/tddy-tools/tests/index_daemon_client_acceptance.rs`
(`refuses_to_warm_without_a_daemon_and_names_the_script_that_starts_one`: a `format!` whose arguments fit on
one line). The file is the approved red-phase suite, which the green phase must not edit, so the diff was left.

**What closes it.** Run `cargo fmt -p tddy-tools` and commit the result as a `style` commit, the way
`3cdca687` did for the move suites. Nothing else in the three packages E3 touches has a fmt diff.
