# 2026-09-09 — CRAP scores coverage as a boolean, and a third of functions miss the join

**Category:** Future enhancement
**Source:** `analyze-coverage-export-and-harness-selection` (#466), first full `tddy-daemon` run

Two separate limits in the same join, both found while reading the first CRAP report the repo has
produced. Neither was touched by #466, which fixed why the report was empty rather than what it says.

- **Coverage is a boolean.** `crap.rs:66-69` sets `covered: record.count > 0` and passes
  `if covered { 1.0 } else { 0.0 }`, so `CRAP = complexity² × (1 − coverage)³ + complexity` collapses
  to `complexity` for anything executed even once and `complexity² + complexity` for anything never
  executed. There is no partial-coverage middle. The consequence is that the leaderboard ranks
  *never-executed* functions strictly by complexity — every score in the daemon's top 50 is exactly
  `cx² + cx` — and a 2,000-line function entered by one test scores the same as a fully covered one.
  [rust-code-analysis.md](../../ft/coder/rust-code-analysis.md) states the ratio formula without
  saying the ratio is only ever 0 or 1, so the docs read as more than the code does.
- **2,999 of 9,064 instrumented functions (33%) fail the `(file, declaration line)` join** on
  `tddy-daemon`; 53 of 127 on `tddy-code-analysis`. Most are likely closures, which the `syn` walker
  scores as independent functions while llvm-cov declares them at the enclosing function's line. Until
  that is understood, the reported join rate (66.9%) is a mix of genuine path mismatches and this
  systematic offset, so it cannot be read as the health signal
  [`analyze-code-issues`](../../../.agents/skills/analyze-code-issues/SKILL.md) treats it as.

Fixing the first needs a real per-function coverage ratio — region hit counts weighted by region
count, which `rust-coverage-final.json` already carries — not just a different constant.
