# Changeset: Rust Code Analysis and Restructuring

**Date**: 2026-08-31
**Status**: 🚧 In Progress
**Type**: Feature

## Plan Mode Discussion (preserved)

See approved plan: Rust Code Analysis and Restructuring — port qape-hq Rust `code-analysis` and `code-restructuring` into `tddy-code-analysis` and `tddy-code-restructuring`, expose via `tddy-tools analyze` / `tddy-tools restructure`, reuse `tddy-lsp` for rust-analyzer assists, import skills under `.agents/skills/` only.

**PRD:** [docs/ft/coder/1-WIP/PRD-2026-08-31-rust-code-analysis-and-restructuring.md](../../ft/coder/1-WIP/PRD-2026-08-31-rust-code-analysis-and-restructuring.md)

## Affected Packages

- **tddy-code-analysis**: [README.md](../../packages/tddy-code-analysis/README.md) — new crate
- **tddy-code-restructuring**: [README.md](../../packages/tddy-code-restructuring/README.md) — new crate (generic core + Rust backend)
- **tddy-lsp**: [docs/changesets.md](../../packages/tddy-lsp/docs/changesets.md) — `request_raw` / `notify_raw` for restructuring bridge
- **tddy-tools**: [docs/changesets.md](../../packages/tddy-tools/docs/changesets.md) — `analyze` / `restructure` CLI
- **repo root**: [Cargo.toml](../../Cargo.toml), [flake.nix](../../flake.nix), `.agents/skills/`

## Related Feature Documentation

- [Reusable LSP](../../ft/coder/reusable-lsp.md)
- [Feature prompt: agent skills](../../ft/coder/feature-prompt-agent-skills.md)
- [tddy-build](../../ft/build/tddy-build.md)

## Summary

Add deterministic Rust CRAP analysis (llvm-cov + syn complexity + HTML report + duplicate tests) and plan-driven restructuring (JSONL intents via rust-analyzer through `tddy-lsp`) as library crates behind `tddy-tools` subcommands.

## Scope

- [x] **Package Documentation**: READMEs (AGENTS.md / architecture notes at wrap)
- [x] **Implementation**: tddy-code-analysis, tddy-code-restructuring, tddy-lsp bridge, tddy-tools CLI
- [x] **Testing**: Acceptance + unit tests for new crates pass
- [x] **Integration**: llvm-tools-preview in flake.nix, skills under `.agents/skills/`
- [ ] **Technical Debt**: full tddy-lsp assist-grade typed API (codeAction/rename/semanticTokens/progress forwarding)
- [x] **Code Quality**: fmt, clippy on new packages

## Technical Changes

### State A (Current)

No Rust code analysis or restructuring in tddy. `tddy-lsp` supports diagnostics/definition/references/hover/symbols only. No `tddy-tools analyze` or `restructure`.

### State B (Target)

Two new workspace crates; `tddy-tools analyze {coverage,report,duplicate-tests}` and `tddy-tools restructure {apply,status,check,anchors,verify}`; `tddy-lsp` extended for restructuring via raw RPC bridge; agent skills `analyze-code-issues` and `code-restructuring` under `.agents/skills/`.

## Implementation Milestones

- [x] Workspace scaffolding (crates, Cargo.toml, flake llvm-tools)
- [x] tddy-code-analysis: complexity, CRAP, coverage, report, duplicate-tests
- [x] tddy-lsp: raw RPC surface for restructuring
- [x] tddy-code-restructuring: generic core + Rust backend on tddy-lsp bridge
- [x] tddy-tools CLI wiring + acceptance tests
- [x] Agent skills imported

## Acceptance Tests

### tddy-code-analysis
- [x] **Unit**: CRAP formula and join on declaration line (`crap.rs`)
- [x] **Unit**: duplicate signature bitset containment (`duplicate_tests.rs`)
- [ ] **Integration**: coverage + report on fixture crate (needs llvm-cov in environment)

### tddy-code-restructuring
- [x] **Unit**: plan refuses code-bearing fields (`plan.rs` — ported)
- [x] **Integration**: static `check` tier + 214 rust backend unit tests

### tddy-tools
- [x] **Acceptance**: `analyze report` fails without coverage (`analyze_cli_acceptance.rs`)
- [x] **Acceptance**: `restructure check` on malformed plan (`restructure_cli_acceptance.rs`)

## Validation Results

- `cargo test -p tddy-code-analysis -p tddy-code-restructuring -p tddy-lsp`: pass
- `cargo test -p tddy-tools --test analyze_cli_acceptance --test restructure_cli_acceptance`: 4/4 pass
- `cargo clippy -p tddy-code-analysis -p tddy-code-restructuring -p tddy-tools -p tddy-lsp -- -D warnings`: pass
- Full `./verify`: failed with **disk full** (`errno=28`) on unrelated packages — not caused by this changeset
