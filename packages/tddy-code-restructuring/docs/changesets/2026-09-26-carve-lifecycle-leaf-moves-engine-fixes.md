# 2026-09-26 — Seven fixes found by the lifecycle leaf moves

**Type:** Fix

`#carve` 15/21 ([#526](https://github.com/uppin/tddy-coder/pull/526)); cross-package entry, with the
moves that found each defect: [2026-09-26-carve-lifecycle-leaf-moves.md](../../../../docs/dev/changesets/2026-09-26-carve-lifecycle-leaf-moves.md). Each was fixed
test-first in its own commit; the crate's tests went from 646 to 694 passed, 0 failed.

- **`check`'s partial-cluster finding reports the edge `apply` refuses** (`3c8323e6`): a moved
  module whose header names a module staying behind, read with `apply`'s own re-export resolution.
  A module staying behind that names the moved one is no longer a finding.
- **Staying behind is read at each operation's point in the plan** (`ebeb8282`), so a mutual set
  spread over separate `move_module_to_crate`s is reported at its first operation, with
  `move_cluster_to_crate` as the remedy. `reports_nothing_when_the_whole_cluster_moves` became
  `reports_a_whole_set_spread_over_separate_moves_at_its_first_operation`.
- **The readiness wait ends on `inactive-code`** (`5446cec6`): a caller survey takes the server's
  empty answer for an item under a switched-off `#[cfg]`; an operation at such code is refused.
- **`'_` is not an untyped placeholder** (`cb367ac6`) in an extracted signature.
- **`extract_variable` renames the binding rust-analyzer introduced** (`cc19d3a4`,
  `backends/rust/introduced.rs`); the untyped-`_` check applies to signatures only.
- **A `return` is allowed in an `extract_method` range that runs to a named `fn`'s tail
  expression** (`ff73fcb6`).
- **A file in no module tree (`unlinked-file`) is refused instead of waited on** (`841545dd`).

New suites: `extract_variable_acceptance`, `unlinked_file_acceptance` (both in nextest's
`rust-analyzer` group). See [readiness-and-gates.md](../readiness-and-gates.md) and
[assist-output-repairs.md](../assist-output-repairs.md#naming-the-binding-extract_variable-introduced).

Code issues: `refusal-move-module-to-crate-any-caller-left-behind` **deleted**, closed by
`3c8323e6` (final measurement in the cross-package entry: plans `01a` and `02a`, 3 and 2 findings
→ 0). `oversized-file-backends-rust` regressed, 4,342 → 4,360 production lines (+18); `facade_lines` unchanged
at 47 lines. `crate_move/cluster.rs` crossed the budget, 493 → 611 production lines, with no record
yet.
