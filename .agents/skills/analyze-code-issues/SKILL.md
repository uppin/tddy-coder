---
name: analyze-code-issues
description: Run Rust CRAP analysis and duplicate-test detection before restructuring via tddy-tools analyze. Use when choosing refactor targets or when the user asks about high-risk untested Rust code.
---

# Analyze Code Issues (Rust)

Tool-backed targeting pass that feeds [`code-restructuring`](code-restructuring/SKILL.md). Complements `/analyze-clean-code` (LLM heuristic); does not replace it.

**v1 scope:** Rust only — complexity + CRAP, coverage capture, HTML report, duplicate-tests. No TypeScript, hot-files, large-files, or `issues` intersection CLI.

## When to use

- Before `code-restructuring` (mandatory prerequisite)
- When choosing which Rust modules to split first
- When the user asks about CRAP or duplicate test signatures

## Workflow

### 1. Identify the target

Rust crate path (directory with `Cargo.toml`). State scope in the output.

### 2. Establish a green baseline

```bash
./test -p <crate>
```

Do not analyze a red tree. Record pass/fail counts.

### 3. Collect coverage

From the repo root (nix dev shell required — `llvm-cov` / `llvm-profdata` on PATH):

```bash
tddy-tools analyze coverage --path packages/<crate> --coverage-dir coverage
```

Produces:

```
coverage/
  rust-coverage-final.json
  per-test/<id>.meta.json
  per-test/<id>.rust.json
```

### 4. Generate CRAP report

```bash
tddy-tools analyze report --path packages/<crate> --coverage-dir coverage
```

Writes `coverage/report.html` and prints join rate on stderr. Missing artifacts exit non-zero.

### 5. Duplicate tests (optional)

```bash
tddy-tools analyze duplicate-tests --coverage-dir coverage
```

Writes `coverage/duplicate-tests/duplicate-tests.html` and `subset-tests.html`.

### 6. Produce a targeting note

Summarize for the developer:

- Top CRAP functions from `report.html` (complex **and** untested)
- Join rate from stderr (low join → complexity/coverage path mismatch)
- Duplicate signature groups (if run)
- **Hand off** — do not start restructuring in this skill

## CRAP formula

`CRAP = complexity² × (1 − coverage)³ + complexity`

Join key is `(file, declaration line)`, never function name.

## References

- [Rust code analysis](../../docs/ft/coder/rust-code-analysis.md)
- [`tddy-code-analysis` README](../../packages/tddy-code-analysis/README.md)
