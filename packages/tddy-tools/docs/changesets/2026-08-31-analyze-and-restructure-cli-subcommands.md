# 2026-08-31 — `analyze` and `restructure` CLI subcommands

**Type:** Feature

`tddy-tools analyze {coverage,report,duplicate-tests}` dispatches to `tddy-code-analysis`; `tddy-tools restructure {apply,status,check,anchors,verify}` dispatches to `tddy-code-restructuring` with `LspRegistry::rust_only()` when LSP is needed. Acceptance tests: `analyze_cli_acceptance`, `restructure_cli_acceptance`. Feature [rust-code-analysis.md](../../../../docs/ft/coder/rust-code-analysis.md), [rust-code-restructuring.md](../../../../docs/ft/coder/rust-code-restructuring.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools)
