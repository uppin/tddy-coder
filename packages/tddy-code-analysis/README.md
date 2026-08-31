# tddy-code-analysis

Rust code analysis: cyclomatic complexity (`syn`), CRAP scoring, llvm-cov per-test capture, HTML reports, duplicate-test detection.

## CLI

Exposed via `tddy-tools analyze`:

- `coverage --path <crate> [--coverage-dir <dir>]`
- `report --path <crate> --coverage-dir <dir>`
- `duplicate-tests --coverage-dir <dir> [--out <dir>]`

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
