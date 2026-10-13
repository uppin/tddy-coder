# 2026-10-13 — the two `extract_method` live suites run in the e2e leg, one rust-analyzer at a time

**Type:** Architecture

`#reshape` 4/19 (PR [#601](https://github.com/uppin/tddy-coder/pull/601)). The code change is
[the package's entry](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-13-reshape-extract-method-clean.md);
this one records the repo-level test configuration and the backlog it cleared.

- **`.config/rust-e2e.filterset`:** `or binary(extract_method_clean_acceptance)` and
  `or binary(extract_method_guard_lift_acceptance)`.
- **`.config/nextest.toml`:** the same two binaries in the `rust-analyzer` serial group's filter, so the
  e2e leg runs them one rust-analyzer at a time.
- **Backlog deleted:** `2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures`,
  `2026-09-24-restructure-apply-leaves-the-lint-gate-red`.
  **Narrowed:** `2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run` (gaps I and K).
  **Filed:** `2026-10-09-restructure-compiler-guided-import-repair`,
  `2026-10-09-restructure-extract-method-lifts-guard-runs-without-outputs-only`,
  `2026-10-09-restructure-extract-method-rust-analyzer-rewrites-that-do-not-build`,
  `2026-10-09-restructure-named-trait-import-used-only-for-methods`,
  `2026-10-09-index-daemon-does-not-see-a-file-edited-outside-a-run`.
- **Consumers in the stack:** `#reshape` 16 (`fn-sizes-rest`) uses the guard lift for
  `plan/codec.rs::parse_op`, tail position for `plan_store/refresh.rs::refreshed`, and the return-type
  spelling for its `?` seams; `#reshape` 19 (`fn-sizes-backend`) uses tail ranges, the return-type
  spelling and one guard lift (`retarget_impl`).
