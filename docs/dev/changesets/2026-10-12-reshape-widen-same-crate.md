# 2026-10-12 — every live rust-analyzer suite of `tddy-code-restructuring` runs in the e2e leg, one server at a time

**Type:** Architecture

`#reshape` 1/19 (PR [#598](https://github.com/uppin/tddy-coder/pull/598)). The code change is
[the package's entry](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-12-reshape-widen-same-crate.md);
this one records the repo-level test configuration.

- **`.config/rust-e2e.filterset`:** 13 live suites added, including the new `same_crate_widening_acceptance`.
- **`.config/nextest.toml`:** 32 suites added to the `rust-analyzer` serial group, so the e2e leg runs them
  one at a time. `apply_tidy_acceptance` and `apply_compile_gate_acceptance` are e2e but not in the group:
  they run `cargo`, not rust-analyzer.
- **Rule:** a binary is live when it reaches `harness::a_rust_analyzer_rooted_at`. Three suites that define
  a local `refusal_of`/`resolving` over text are not (`repoint_call_plan_acceptance`,
  `repoint_facade_imports_acceptance`, `retarget_impl_plan_lines`). Every node that adds a live suite adds
  it to both files. Checked by `bun test ./scripts/nextest-serial-groups.test.ts` (22 pass).
