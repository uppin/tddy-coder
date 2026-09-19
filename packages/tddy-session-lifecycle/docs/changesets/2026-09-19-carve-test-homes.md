# 2026-09-19 — The crate gets its own tests

**Type:** Refactor

`#carve` 4/10 ([#498](https://github.com/uppin/tddy-coder/pull/498)). Cross-package entry:
[2026-09-19-carve-test-homes.md](../../../../docs/dev/changesets/2026-09-19-carve-test-homes.md).

This crate had **no `tests/` directory at all** over 31,700 production lines. Its acceptance
coverage existed and passed — it just sat in `packages/tddy-daemon/tests/`, reaching this code
through a facade that re-exported 82 of these modules. `./test -p tddy-session-lifecycle` therefore
proved almost nothing about the crate, and nothing inside the crate showed that.

**95 suites and a shared `tests/common/mod.rs` now live here** (96 renames in all), moved with
`move_test_binary_to_crate` and re-pointed at the crates that *define* what they reach rather than
at whichever one re-exported it. `[dev-dependencies]` gained the crates those suites name. No
assertion changed: the diffs are crate-path rewrites and rustfmt reflow of `use` groups. Two suites
that read a sibling crate's source through a `CARGO_MANIFEST_DIR`-relative **string** were hand
edited, because string literals are deliberately never rewritten by the move.

What belongs here and how to write one: [test-suites.md](../test-suites.md).

**Closes** `missing-tests-crate-has-no-test-directory.md` — first detected 2026-09-15 at **0** test
binaries with 97 suites / 38,629 lines elsewhere; at wrap, **95** test binaries and a `tests/common/`
in the crate.
