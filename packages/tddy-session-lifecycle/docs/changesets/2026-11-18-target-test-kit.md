# 2026-11-18 — Acceptance tests use the shared stand-in target kit

**Type:** Refactor (tests) · `#coder-start` 9/10, PR [#949](https://github.com/uppin/tddy-coder/pull/949)

Local `write_executable` (and, where identical, the ssh stand-in target) replaced by
`tddy_testing_commons::remote_target`; assertions unchanged.
