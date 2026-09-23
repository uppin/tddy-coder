# 2026-09-23 — Three carved crates dev-depend back on `tddy-core` through `tddy-testing-commons`

**Category:** Future enhancement — build cost
**Source:** `#carve` 12/14 (`carve-core-facade`, PR #522), found at green

`tddy-changeset`, `tddy-session-actions` and `tddy-workflow-engine` have a `[dev-dependencies]`
entry on `tddy-testing-commons`, because the suites that moved with them call its
`temp_session_dir`. `tddy-testing-commons` depends on `tddy-core`, and `tddy-core` re-exports all
three. So each of these crates' **test** builds compiles `tddy-core` and every crate it re-exports,
including the crate under test a second time as a dependency of the facade.

Cargo permits this, since a dev-dependency cycle is not a build cycle. #522's shape test
(`packages/tddy-core/tests/core_facade_shape.rs`, `no_receiving_crate_depends_on_tddy_core`) reads
normal dependencies only, so it does not see the edge. Nothing is wrong at runtime. The cost is the
compile time the carve was meant to save, and a type-identity trap: a test that mixes a value built
through `tddy_testing_commons` → `tddy_core` with one built by the crate under test can see two
distinct copies of the same type.

**What would close it.** Move `temp_session_dir`, and whatever else these suites use, into a helper
that does not depend on `tddy-core`. That could be a small module in `tddy-testing-commons` split
from its `tddy-core`-dependent part, or a `test_support` module in the crate. Then drop the
dev-dependency. Related: `2026-09-10-the-dependency-boundary-harness-is-duplicated-per-crate.md`.
