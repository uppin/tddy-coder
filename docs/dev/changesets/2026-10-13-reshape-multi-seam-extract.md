# 2026-10-13 — the multi-seam live suite runs in the e2e leg, one server at a time

**Type:** Architecture

`#reshape` 2/19 (PR [#599](https://github.com/uppin/tddy-coder/pull/599)). The code change is
[the package's entry](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-13-reshape-multi-seam-extract.md);
this one records the repo-level test configuration.

- **`.config/rust-e2e.filterset`:** `or binary(multi_seam_extract_acceptance)`.
- **`.config/nextest.toml`:** the same binary in the `rust-analyzer` serial group's filter, so the e2e
  leg runs it one rust-analyzer at a time.
- Checked by `bun test ./scripts/nextest-serial-groups.test.ts` (22 pass).
