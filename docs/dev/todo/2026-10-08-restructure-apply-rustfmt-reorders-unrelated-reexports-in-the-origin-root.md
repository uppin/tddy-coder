# 2026-10-08 — an apply's `rustfmt` pass reorders unrelated `pub use` lines in the origin's `lib.rs`

**Category:** Restructure engine defect (reviewability of the diff, not correctness)
**Source:** #carve 21/21 (PR #536), R1: `move_module_to_crate` of `agent_list_mapping` from
`tddy-session-lifecycle` to `tddy-daemon-kernel`. Related: the `rustfmt`-over-touched-files pass in
[2026-09-24-restructure-apply-leaves-the-lint-gate-red](2026-09-24-restructure-apply-leaves-the-lint-gate-red.md)

## What the engine did

The plan moved one module and wrote one facade. The apply's `rustfmt` over `packages/tddy-session-lifecycle/src/lib.rs`
also sorted the file's other root-level items, because `lib.rs` was not `rustfmt`-sorted before. The diff of that
file is 17 lines for a one-line change: `pub use tddy_daemon_sandbox::*;` and
`pub use tddy_telegram::active_elicitation;` (with its three-line `///` comment) changed places, and
`pub use tddy_daemon_kernel::config;` became `pub use tddy_daemon_kernel::{agent_list_mapping, config};`
(the last is the facade and is right).

Nothing behaves differently; the doc comment still sits on the item it described.

## What was needed

Only the facade line. No hand edit was made: reverting the reorder by hand would be a second change on top of the
engine's, and the next apply's `rustfmt` would redo it.

## Why deferred

The cost is a noisier diff in a file every later milestone of #536 touches (`lib.rs` carries the crate's facades), and
the fix is in the engine: format only the lines an operation wrote, or sort the file once, in its own commit, before
the plan runs. Reviewers of this node should read `lib.rs` as "one facade, plus a reorder".
