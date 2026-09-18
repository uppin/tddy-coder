# missing-tests: the crate has no `tests/` directory at all

**Location:** `packages/tddy-session-lifecycle/`
**Category:** missing-tests
**Detected:** 2026-09-15 by structural audit
**Metrics:** **0** test binaries in this crate · **97 suites / 38,629 lines** of its acceptance coverage live in `tddy-daemon`
**Restructure:** required — relocate the test binaries, which needs a new restructure operation
**Status:** Open — claimed by #498, in flight
**Claimed by:** #498 — `#carve` 4/10 `test-homes` · draft · `feature/carve/test-homes`
**Lands after:** #488, #489, #490

## Measurement history

| Run | Own test binaries | Suites elsewhere | Note |
|---|---|---|---|
| 2026-09-15 | 0 | 97 | first detection |

## What the tool found

This crate is **31,700 production lines with no `tests/` directory**. Its acceptance coverage exists
— 97 suites, 38,629 lines — but sits in `packages/tddy-daemon/tests/`, reaching this code through a
facade in `tddy-daemon/src/lib.rs` that re-exports 82 of this crate's modules.

Counted independently: **0 of 139** files in `tddy-daemon/tests/` name `tddy_session_lifecycle`;
**133** name `tddy_daemon::`.

## Why it matters here

`./test -p tddy-session-lifecycle` proves almost nothing about the crate — a fact that is invisible
from inside it, which is the worst property a coverage gap can have. The crate looks untested and is
not; it is untestable **in place**.

## What would close it

Move the 97 suites here. That needs a `move_test_binary_to_crate` operation, because
`move_module_to_crate` refuses any anchor outside `<crate>/src/`. Then `tddy-daemon`'s facade can go.

This is not a "write more tests" issue — the tests exist and pass. It is a placement issue, which is
why it closes by relocation rather than by authoring.

## If you are about to change this code

#498 adds a `tests/` directory and 97 files to this crate. It **adds no new test and changes no
assertion**, so a concurrent change is unaffected unless it also adds a test binary here — in which
case say so, since the destination directory does not exist yet.

If you are adding coverage for this crate **now**, put it in `tddy-daemon/tests/` where its siblings
are, and #498 will move it with them. Creating `packages/tddy-session-lifecycle/tests/` ahead of #498
is the one thing to avoid.

## Verified by hand

2026-09-15: confirmed no `tests/` directory exists; resolved all 139 `tddy-daemon` test files through
both facade hops to establish the 97 figure. Confirmed the crate's inline tests (6,337 lines) are
unit-level and crate-local.
