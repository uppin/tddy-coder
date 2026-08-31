# tddy-code-analysis

Rust code analysis library: cyclomatic complexity (`syn`), CRAP scoring, llvm-cov per-test capture, HTML reports, duplicate-test detection.

**Feature doc:** [docs/ft/coder/rust-code-analysis.md](../../docs/ft/coder/rust-code-analysis.md)

## CLI

Exposed via `tddy-tools analyze`:

- `coverage --path <crate> [--coverage-dir <dir>]`
- `report --path <crate> --coverage-dir <dir>`
- `duplicate-tests --coverage-dir <dir> [--out <dir>] [--min-signature <n>] [--subset-ratio <r>] [--include-test-sources]`

`report` and `duplicate-tests` fail with a clear error when coverage artifacts are missing.

## Artifacts

```
coverage/
  rust-coverage-final.json
  report.html
  per-test/<id>.meta.json
  per-test/<id>.rust.json
  duplicate-tests/duplicate-tests.html
  duplicate-tests/subset-tests.html
```

## CRAP join

Functions join on `(file, declaration line)`:

`CRAP = complexity² × (1 − coverage)³ + complexity`

Coverage is binary per function (entered or not).
