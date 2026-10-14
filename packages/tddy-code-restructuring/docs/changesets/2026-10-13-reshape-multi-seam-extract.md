# 2026-10-13 — a multi-seam plan resolves every operation against the files the plan has already written

**Type:** Fix (check/apply parity for multi-seam `extract_module` plans; no new operation, field, flag or wire message)

`#reshape` 2/19 (PR [#599](https://github.com/uppin/tddy-coder/pull/599), `feature/reshape/multi-seam-extract`,
no parent). Product entry:
[2026-10-13-reshape-multi-seam-extract.md](../../../../docs/ft/coder/changelog/2026-10-13-reshape-multi-seam-extract.md).
How it works: [assist-output-repairs.md](../assist-output-repairs.md#a-seam-after-a-seam),
[readiness-and-gates.md](../readiness-and-gates.md#documents-are-closed-when-an-operation-ends).

## What changed

- **`backends/rust/projection.rs` (new).** `PlanProjection` records the files the backend's resolutions
  created, changed or renamed into, in first-written order; `stage_projection` reads them through the
  workspace at the start of `resolve`; `open_staged_projection` opens them before the operation's first
  document, except the one being opened. A re-open of a projected uri is `didChange`, not a second
  `didOpen`. An unreadable recorded path is an error naming it. `closing_what_it_opens` closes them and
  clears the staging. The first operation of a run sends what it sent before.
- **`backends/rust.rs`**: `mod projection;`, the `projection` and `staged_projection` fields in both
  constructors, and `resolve` (stage, then resolve inside `closing_what_it_opens`, then record the edit
  on `Ok`). `backends/rust/documents.rs`: `did_open` flushes the staged projection; `close_opened` drops it.
- **`runner/rehearsal.rs`, `runner/entry_points/check_entry_points.rs`**: `Rehearsed.widenings` carries
  `Resolution::report`; `rehearsed_lines` returns the survey lines, one `visibility:` line per widening,
  then the notes; `check_plan` prints through it. `check --deep` now prints the widenings `apply` prints.
- **`backends/rust/visibility.rs`**: `restore_visibility` no longer reports an item already written
  `pub(crate)` as widened (`widening_of`, with three unit tests). **Approved exception to the boundary
  that left `restore_visibility` unchanged** (developer decision 2026-10-12): without it a dry run and
  a deep check printed `` `PtyHandle` pub(crate) -> pub(crate) ``. The report construction moved into
  `widening_of`, so the function is 8 lines shorter than on master.
- **Tests**: `tests/multi_seam_extract_acceptance.rs` (new live binary, 9 tests) with fixtures in
  `tests/harness/mod.rs`; unit tests in `projection.rs` and `rehearsal.rs`.
- **Registration**: the binary is in `.config/rust-e2e.filterset` and the `rust-analyzer` group of
  `.config/nextest.toml`. These two files are repo-level, so the change has a cross-package entry too:
  `docs/dev/changesets/2026-10-13-reshape-multi-seam-extract.md`.

## Backlog entries

- Closed, deleted: `2026-09-18-extract-module-cannot-see-sibling-seams-in-one-plan`. Rows 2 and 3 and
  the parity claim are covered by acceptance tests 1, 3, 4 and 8. Row 1's sibling-file half is closed;
  its in-body half was already fixed by `inline_paths`. Row 2 (helpers left unqualified in a later seam)
  did not reproduce on the current engine and is held by pins 6 and 7. The third "possible fix",
  type-checking the projected tree, is the new entry below.
- Narrowed, kept: `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run`. J closed
  here (the same root cause as row 3 above; tests 3-5); H removed as fixed (`inline_paths`); X's second
  half moved out. Remaining: G, I, K, L, M, W and X's first half.
- New: `2026-10-09-restructure-check-deep-does-not-compile-the-projected-tree`,
  `2026-10-09-restructure-extract-module-widens-to-pub-crate-where-pub-super-suffices`,
  `2026-10-09-restructure-projection-does-not-hide-files-renamed-away-in-a-rehearsal`.
- Touched, not fixed (`rust.rs` wiring only): `2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node`.

## Verification (scoped)

`./test -p tddy-code-restructuring`: **1381 passed, 0 failed, 1 ignored** (a doc-test that is `ignore`
by design). `multi_seam_extract_acceptance`, live rust-analyzer: **9/9** (4 red on `master`, 5 pins).
`cargo clippy -p tddy-code-restructuring --all-targets -- -D warnings` and `cargo fmt` clean.
`bun test ./scripts/nextest-serial-groups.test.ts`: 22 pass, 0 fail. Whole-workspace health is CI's,
read after the wrap.

## Deferred, with the developer's approval (2026-10-13)

**The `rust.rs` file-length decomposition.** `backends/rust.rs` grew by 15 production lines (2,983 to
2,998 counted to the first inline `#[cfg(test)] mod`, the record's own rule; the gate's count, which stops at the first `#[cfg(test)]`, reads 510 to 517),
wiring only, against a 500-line budget; the logic is in `projection.rs` and `documents.rs`. Splitting
it is deferred: the impl-member seams still need an engine move, and later `#reshape` nodes edit this
file. The record `oversized-file-backends-rust` carries the measurement row and stays open, unclaimed.
No backlog entry was added, by the developer's choice.

## Code issues (re-measured 2026-10-13)

None was claimed by #599. Production lines are counted to the first inline `#[cfg(test)] mod`.

| Record | Measurement | Verdict |
|---|---|---|
| `oversized-file-backends-rust` | 2,998 (+15 over 2,983 at the merge base) | worse by the wiring above; row recorded; open, unclaimed |
| `complexity-rust-facade-lines` | `facade.rs` not touched | unchanged, left as is |
| `dead-code-plan-filehint-modified` | `plan.rs` not touched | unchanged, left as is |
| `oversized-file-test-binary` | `crate_move/test_binary.rs` not touched | unchanged, left as is |
| `broken-restructure-anchors-empty-outline` | `anchors` not touched | unchanged, left as is |

Other touched files stay under 500 production lines: `projection.rs` 91, `documents.rs` 76,
`rehearsal.rs` 181, `check_entry_points.rs` 377. No function over 60 lines grew.
