# Rust Code Analysis (CRAP, coverage, duplicate tests)

**Product area:** Coder / tddy-tools  
**Status:** Active  
**Updated:** 2026-09-16

## Summary

`tddy-tools analyze` provides deterministic Rust code quality targeting: llvm-cov per-test coverage capture, cyclomatic complexity via `syn`, CRAP scoring joined on `(file, declaration line)`, HTML reports, and duplicate/subset test detection from per-test region signatures. The library crate is `tddy-code-analysis`; there is no separate binary.

**v1 scope:** Rust only. No TypeScript, `hot-files`, large-file ranking, or `issues` intersection CLI. Coverage is invoked through `tddy-tools analyze`, not `tddy-build`. No CI gate on CRAP or coverage thresholds.

Complements `/analyze-clean-code` (LLM heuristic at PR-wrap). Run analysis **before** plan-driven restructuring.

## Entry points

Two front ends over one engine.

| Front end | Shape |
|---|---|
| `tddy-tools analyze …` | one operation per process, printing to a console |
| `tddy-index-daemon` | `Coverage`, `Report`, `DuplicateTests` and `Complexity` on `code_index.CodeIndexService`, over gRPC and stdio |

`Coverage` and `DuplicateTests` are **server-streaming**, because they are tens of minutes of
near-silence otherwise — 55.8 minutes for a `tddy-daemon` capture, ~22 for duplicate-tests — and the
phase a run is in is the only thing that distinguishes it from a hang. `Complexity` is unary: it is
the one analysis operation cheap enough to be, and cheaper still on a warm process.

**A capture cannot yet be stopped.** `capture_coverage` takes no cancellation surface, so a client
that hangs up one minute in leaves the other 54 running; the progress sink notices the dropped
receiver and stops sending, but cannot cancel. See
[PRD: warm code-intelligence daemon](1-WIP/PRD-2026-09-15-warm-code-intelligence-daemon.md).

## Complexity is cached by content, not by path

`file_complexity` is pure over the source text, so a warm process keeps each score against a hash of
the text it scored. Two files with identical contents are scored once; a file whose contents changed
is always rescored; and a path is never the key, because a path says nothing about whether the answer
is still true.

The command line installs a **pass-through** cache that keeps nothing, so `tddy-tools analyze`
behaves exactly as it always has. The cache is process-wide rather than per workspace root: content
addressing makes a per-root partition both unnecessary and wasteful across worktrees of one repo.

It is **unbounded** — keyed by content, so over a tree under active edit it grows with the number of
*versions* of a file rather than the number of files. It needs an eviction policy.

## CLI

```text
tddy-tools analyze coverage --path <crate-or-package> [--coverage-dir <dir>]
tddy-tools analyze report --path <crate-or-package> --coverage-dir <dir>
tddy-tools analyze duplicate-tests --coverage-dir <dir>
             [--out <dir>] [--min-signature <n>] [--subset-ratio <r>] [--include-test-sources]
```

`report` and `duplicate-tests` require coverage artifacts. If they are missing, the command fails with an instruction to run `analyze coverage` first (no silent empty report).

## Coverage capture

Given a Rust crate path, `analyze coverage`:

1. Builds instrumented tests (`cargo test --no-run`).
2. Lists tests and runs each with `LLVM_PROFILE_FILE`.
3. Merges profiles and exports JSON via `llvm-cov` / `llvm-profdata`.
4. Writes normalized artifacts under the coverage directory.

**Artifacts:**

```
coverage/
  rust-coverage-final.json     # denominator: all regions + functions per file
  per-test/<id>.meta.json      # test metadata (id, name, spec, line, status, duration)
  per-test/<id>.rust.json      # regions this test executed
  report.html                  # written by `analyze report`
  duplicate-tests/             # written by `analyze duplicate-tests`
    duplicate-tests.html
    subset-tests.html
```

`rust-coverage-final.json` records `functions: [{ name, line, count }]` per source file. Per-test IDs are `md5(spec + "\0" + name)` truncated to 16 hex chars.

The nix dev shell provides `llvm-tools-preview` on the rust-overlay toolchain (`llvm-cov`, `llvm-profdata` on PATH).

## Cyclomatic complexity

A `syn` visitor scores `fn`, methods, and closures independently. Decision points counted: `if`, `while`, `for`, `loop`, `match` arms, `if let`, `while let`, `&&`, `||`, `?`. Complexity is `1 + decision points` per function.

## CRAP scoring

Functions join coverage and complexity on **`(file, declaration line)`**, never function name:

```
CRAP(f) = complexity(f)² × (1 − coverage(f))³ + complexity(f)
```

Coverage is binary per function (entered or not). A covered function scores exactly its complexity. Join rate is reported so source-map/profile drift is visible.

`analyze report` writes `coverage/report.html` (top CRAP leaderboard) and prints a console summary, e.g. `CRAP: N function(s) scored, worst name (score) · join rate X%`.

## Duplicate and subset tests

Per-test signatures union branch and statement region keys (`s:` / `b:` prefixes). Counts are discarded. Exact groups use interned bitsets; strict subsets use an inverted index with configurable `min-signature` and `subset-ratio`.

## Agent skill

[`.agents/skills/analyze-code-issues`](../../../.agents/skills/analyze-code-issues/SKILL.md) — green baseline → `analyze coverage` → `report` → targeting note from the CRAP leaderboard (and duplicate-tests when useful) → **stop**; do not start restructuring.

## Related documentation

- [Rust code restructuring](rust-code-restructuring.md) — plan-driven refactors (run after analysis)
- [Feature prompt: agent skills](feature-prompt-agent-skills.md) — skill discovery
- [tddy-build](../build/tddy-build.md) — no v1 contract change; Rust crates remain analysis targets
- Package: [`packages/tddy-code-analysis/README.md`](../../../packages/tddy-code-analysis/README.md)

## Known limitations

- Duplicate-test comparison is within one coverage run.
- Binary per-function coverage makes CRAP coarser than a line-weighted variant.
- End-to-end coverage capture on a fixture crate is not yet exercised in CI (unit tests cover CRAP join and bitset logic).
