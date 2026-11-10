# complexity: UsersMapping::from_yaml

**Location:** `packages/tddy-users-file/src/lib.rs:98` — `UsersMapping::from_yaml`
**Category:** complexity
**Detected:** 2026-11-10, `#agent-tools` sweep (#876) structural re-measurement
**Metrics:** **41 lines** (2026-11-10) · **nesting depth about 6** · 8 branch/match lines · 5 early `return`s · 1 parameter
**Thresholds breached:** nesting about 6 > 4 (`/analyze-clean-code`); length within budget
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred by the user, 2026-11-10 (#876 sweep); unclaimed

Deferred by the user, 2026-11-10 (#876 sweep): the restructure engine refused the seam (missing imports for builder types / early returns in the range); revisit after the engine fix

## Measurement history

| Run | Lines | Nesting | Note |
|---|---|---|---|
| 2026-11-10 | 41 | about 6 | The row loop nests the empty/root/unsafe checks and the `BTreeMap` entry match; every seam that would flatten it holds an early `return Err(…)`. The supervisor's copy of these checks was replaced by this crate in #876 (B8), which is when the depth arrived here. |

## What would close it

Extract the per-row check (or the one-OS-user-per-login bookkeeping) once the engine handles an
early return in the range. Anchor with `tddy-tools restructure anchors`, never by hand, then
prove the seam with `restructure check --deep` against a warm index.
