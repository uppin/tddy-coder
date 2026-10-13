# 2026-10-13 — the extract-facade live suite runs in the e2e leg, one server at a time

**Type:** Architecture

`#reshape` 3/19 (PR [#600](https://github.com/uppin/tddy-coder/pull/600)). The code change is
[the package's entry](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-13-reshape-tidy-facades.md);
this one records the repo-level test configuration and the backlog it cleared.

- **`.config/rust-e2e.filterset`:** `or binary(extract_facade_acceptance)`.
- **`.config/nextest.toml`:** the same binary in the `rust-analyzer` serial group's filter, so the e2e leg
  runs it one rust-analyzer at a time.
- **Backlog deleted:** `2026-10-03-restructure-glob-reexport-is-narrower-than-the-moved-items-need`,
  `2026-10-08-restructure-cluster-move-leaves-unused-reexports-and-an-orphan-doc-in-the-origin`,
  `2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root`.
  **Narrowed:** `2026-10-08-restructure-move-item-miswrites-a-facade-path-in-a-moved-impl-block`,
  `2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass`.
