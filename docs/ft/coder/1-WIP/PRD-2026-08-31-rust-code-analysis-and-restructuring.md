# Rust Code Analysis and Restructuring — PRD

**Date**: 2026-08-31
**PRD Type**: Enhancement (new capabilities on existing coder/tools/LSP surfaces)
**Status**: 📋 Planned

## Affected Features

**CRITICAL**: List ALL feature documents affected by this PRD:

- **Primary (new)**: Rust code analysis — CRAP scoring, llvm-cov capture, HTML report, duplicate/subset tests for Rust crates (feature doc produced at wrap: `docs/ft/coder/rust-code-analysis.md`)
- **Primary (new)**: Rust code restructuring — plan-driven, engine-authored refactors via rust-analyzer (feature doc produced at wrap: `docs/ft/coder/rust-code-restructuring.md`)
- **Related**: [Reusable Language Server (LSP) Support](../reusable-lsp.md) — restructuring reuses `tddy-lsp` (no private rust-analyzer spawn); the client must grow the methods assists need (`codeAction` / resolve, `rename`, `semanticTokens/full`, progress, initialize capabilities)
- **Related**: [Feature prompt: project agent skills](../feature-prompt-agent-skills.md) — import `analyze-code-issues` and `code-restructuring` under `.agents/skills/` only (discovered automatically)
- **Related**: [tddy-build](../../build/tddy-build.md) — analysis/restructuring target Rust that `tddy-build-rust` already builds; v1 does **not** add a new `BUILD.yaml` target type or `BuildMode`. Coverage is invoked through `tddy-tools analyze`, not `tddy-tools build`

## Summary

Bring qape-hq’s **Rust** halves of `@wix/code-analysis` and `@wix/code-restructuring` into tddy as two library crates, exposed only as **`tddy-tools` CLI subcommands**. Agents (and developers) get tool-backed CRAP targeting and plan-driven restructuring that never writes moved code by hand. TypeScript is out of scope.

## Background

tddy already builds and tests Rust (`tddy-build-rust`, `tddy-tools build`) and can talk to rust-analyzer (`tddy-lsp` + MCP LSP tools). It has no reproducible quality metric for Rust, no coverage capture, and no way to apply a reviewable refactor **plan** through the language engine.

qape-hq already solved this:

| qape package | Rust-relevant surface |
|---|---|
| `@wix/code-analysis` | `syn` cyclomatic complexity, CRAP join on `(file, declaration line)`, HTML report, duplicate/subset tests from per-test llvm regions |
| `@wix/code-restructuring` | JSONL intent plan → rust-analyzer assists → journalled apply (`apply` / `status` / `check` / `anchors` / `verify`) |

`/analyze-clean-code` remains the LLM heuristic at PR-wrap. This work is the **deterministic** targeting pass that should run **before** a restructure, complementary rather than a replacement.

### Decisions locked in planning

- **Rust only.** No TypeScript sidecar, no istanbul, no TS complexity.
- **Two library crates** (`tddy-code-analysis`, `tddy-code-restructuring`). **No new binaries.** CLI is `tddy-tools analyze …` and `tddy-tools restructure …`.
- **Analysis v1**: complexity + CRAP, HTML report, duplicate-tests. Not in v1: `hot-files`, large-file ranking, `issues` orchestration.
- **Coverage capture is in v1** (llvm-cov → `rust-coverage-final.json` and per-test artifacts).
- **Restructuring v1**: full Rust operation set and all five subcommands.
- **Reuse `tddy-lsp`.** Do not spawn a private rust-analyzer.
- **Skills** land only under `.agents/skills/` (tddy discovery), rewritten for tddy CLI/paths.

## Proposed Changes

### What's Changing

#### New crate: `tddy-code-analysis`

Library that, given a Rust crate/package path:

1. **Collects coverage** with llvm instrumentation (one instrumented `cargo test --no-run`, then per-test runs with `LLVM_PROFILE_FILE`, merge + `llvm-cov export`), writing:
   - `coverage/rust-coverage-final.json` — every known region and function record, including unexecuted; `functions: [{ name, line, count }]` per source file
   - `coverage/per-test/<id>.rust.json` + `.meta.json` — regions this test executed
2. **Scores cyclomatic complexity** with a `syn` walker over `fn`, methods, and closures (`if`, `while`, `for`, `loop`, `match` arms, `if let`, `while let`, `&&`, `||`, `?`). Nested functions scored independently.
3. **Joins CRAP** on `(file, declaration line)`, never function name:

   `CRAP(f) = complexity(f)² × (1 − coverage(f))³ + complexity(f)`

   Coverage is binary per function (entered or not). A covered function scores exactly its complexity. Join rate is reported so source-map/profile drift is visible.
4. **Writes `coverage/report.html`** with a Highest CRAP leaderboard (sortable by name / complexity / score) and prints a console summary, e.g. `CRAP: N function(s) scored, worst name (score) · join rate X%`.
5. **Detects duplicate and subset tests** from per-test region signatures (branch regions ∪ statement regions; counts discarded). Exact bitset index; no MinHash. Emits `duplicate-tests.html` and `subset-tests.html`.

Reported only — no CI gate, no score threshold.

#### New crate: `tddy-code-restructuring`

Library that replays a JSONL **plan of named intents** (never source text) against rust-analyzer through `tddy-lsp`:

| Subcommand | Role |
|---|---|
| `apply` | Execute the plan; `--dry-run` rehearses the whole plan in an overlay; `--resume` continues from the journal |
| `status` | completed / in_flight / pending / failed |
| `check` | All findings, no writes; `--deep` resolves through the same path as apply |
| `anchors` | Emit a correct range covering named items (including trivia) |
| `verify` | Statement-multiset comparison against a git ref |

**Rust operations (v1, all required):** `extract_method`, `extract_variable`, `rename_symbol`, `extract_module` (`reexport`: glob / named / none, `to_file`), `extract_module_to_file`, `extract_trait`, `inline_method`.

Invariants carried from qape (behaviour, not implementation):

- A plan must not contain `text` / `code` / `content`, `create_file`, or `insert_text`. Parser refuses them.
- Unsupported operations are hard errors, not skips.
- Files appear because an operation caused them (`to_file` / `extract_module_to_file`), never because a plan declared them.
- Moves that need history use `git mv`.
- Visibility widenings are reviewable output (named on stdout / journal), not silent.
- Progress goes to an injected sink, never mixed into the library’s stdout.

#### `tddy-tools` CLI

No new binaries. New subcommands on the existing tooling binary:

```text
tddy-tools analyze coverage --path <crate-or-package> [--coverage-dir <dir>]
tddy-tools analyze report --path <crate-or-package> --coverage-dir <dir>
tddy-tools analyze duplicate-tests --coverage-dir <dir>
             [--out <dir>] [--min-signature <n>] [--subset-ratio <r>] [--include-test-sources]

tddy-tools restructure apply <plan.jsonl> [--dry-run] [--resume] [--from N] [--stop-after N]
                                       [--indexing-budget SECONDS]
tddy-tools restructure status <plan.jsonl>
tddy-tools restructure check <plan.jsonl> [--deep] [--indexing-budget SECONDS]
tddy-tools restructure anchors <file.rs> --items A,B,C [--indexing-budget SECONDS]
tddy-tools restructure verify --against <git-ref>
```

`analyze report` requires coverage artifacts; if they are missing it fails with an instruction to run `analyze coverage` first (no silent empty report).

#### `tddy-lsp` (reuse, not a second server)

Restructuring must use the existing long-running rust-analyzer task (`tddy-lsp` / `LspRegistry`). The current client is diagnostics / definition / references / hover / symbols / `didOpen`. Assists need additional LSP methods and initialize capabilities (no snippet support; rust-analyzer `initializationOptions` for import granularity and indexing progress). Those are additive on [reusable-lsp](../reusable-lsp.md); the five agent MCP tools stay language-agnostic and unchanged in name.

#### Agent skills (`.agents/skills/` only)

Import and **rewrite** (no `yarn`, no `@wix`, no TypeScript sidecar):

| Skill | Role |
|---|---|
| `analyze-code-issues` | Green baseline → `tddy-tools analyze coverage` → `report` → targeting note from CRAP leaderboard (and duplicate-tests when useful) → **stop**; do not start restructuring |
| `code-restructuring` | Green baseline → run `analyze-code-issues` → plan intents → `restructure check` / `apply` / `verify`; full Rust operation vocabulary |

`/analyze-clean-code` is unchanged.

### What's Staying the Same

- TypeScript analysis and the ts-morph restructure sidecar stay in qape-hq; tddy does not grow them.
- `tddy-build` / `tddy-build-rust` lowering (`rust_binary` / `rust_library`, Compile / Test / Run) is unchanged. No `BuildMode::Coverage`.
- `tddy-tools --mcp` LSP tool names and schemas are unchanged.
- Skill discovery remains `.agents/skills/<name>/SKILL.md` with matching frontmatter `name`.
- `/analyze-clean-code` remains the LLM PR-wrap heuristic.
- No CI gate on CRAP, coverage %, or duplicate tests.

## Impact Analysis

### Technical Impact

- Two new workspace members; `tddy-tools` depends on both libraries (CLI dispatch only).
- `tddy-code-restructuring` depends on `tddy-lsp` for server lifetime and JSON-RPC. Expect `tddy-lsp` API growth (code actions, rename, semantic tokens, `$/progress`, richer `initialize`).
- Dev shell needs LLVM coverage tools (`llvm-tools-preview` on the rust-overlay toolchain, plus `llvm-profdata` / `llvm-cov` available in nix). `cargo-llvm-cov` is not required if the crate drives `llvm-cov export` itself.
- `syn` is a new analysis dependency (already used conceptually in qape’s `rust-cyclomatic-complexity`).
- Restructuring tests that start rust-analyzer are load-sensitive; indexing is paid once per run with a raisable budget (same contract as qape).

### User Impact

- Developers and agents gain two `tddy-tools` command families; `tddy-tools --help` lists them.
- Agents can `/analyze-code-issues` then `/code-restructuring` from the feature-prompt slash menu (valid `.agents/skills` discovery).
- Restructuring still requires a green baseline; a red tree is a stop.
- No change to session start, recipes, or web UI in this PRD.

## Implementation Plan

1. Document this PRD and the technical changeset (Plan mode).
2. `tddy-code-analysis`: complexity + CRAP join + coverage capture + HTML report + duplicate-tests; CLI on `tddy-tools analyze`.
3. Extend `tddy-lsp` with the methods/capabilities the Rust backend needs; keep MCP tool set unchanged.
4. `tddy-code-restructuring`: plan/journal/ledger/overlay + Rust backend on `tddy-lsp`; CLI on `tddy-tools restructure`.
5. Import rewritten skills under `.agents/skills/analyze-code-issues` and `.agents/skills/code-restructuring` (plus plan-schema / changeset references).
6. Acceptance tests against fixture crates (complexity/CRAP join, coverage artifacts, duplicate signatures, extract_module apply/check/verify).
7. Wrap into feature docs `rust-code-analysis.md` and `rust-code-restructuring.md`.

## Acceptance Criteria

- [ ] `tddy-tools analyze coverage --path <rust crate>` produces `rust-coverage-final.json` and per-test artifacts without a second rust-analyzer or a TypeScript toolchain
- [ ] `tddy-tools analyze report` joins CRAP on `(file, declaration line)`, writes `coverage/report.html`, and prints a CRAP console summary including join rate
- [ ] A covered function’s CRAP equals its complexity; an untested complex function ranks above a trivial untested one
- [ ] `tddy-tools analyze duplicate-tests` groups identical signatures and strict subset relations from Rust region keys; reports are HTML under the coverage dir
- [ ] Missing coverage artifacts make `report` / `duplicate-tests` fail with a clear “run analyze coverage first” error (no empty success)
- [ ] `tddy-tools restructure` exposes `apply`, `status`, `check`, `anchors`, `verify` with the qape-equivalent flags
- [ ] All seven Rust operations resolve through rust-analyzer via `tddy-lsp` (one server keyed by workspace root + language, not a private child of the restructure crate)
- [ ] A plan carrying `text` / `code` / `content` is refused; an unsupported op is a hard error
- [ ] `check --deep` predicts the same refusal an `apply` would give; `apply --dry-run` writes nothing
- [ ] `verify --against HEAD` after a successful extract_module reports no missing behavioural statements (trivia-only / `use` / `mod` excluded per qape contract)
- [ ] `.agents/skills/analyze-code-issues` and `.agents/skills/code-restructuring` are valid tddy skills (frontmatter `name` matches folder) and mention only `tddy-tools` + Rust
- [ ] No new binaries; `cargo build -p tddy-tools` is the only CLI artifact
- [ ] TypeScript is not accepted by either crate (`.ts` / sidecar paths refused or absent)

## Constraints and Limitations

### Scope Limitations (v1)

- No TypeScript / JavaScript analysis or restructuring.
- No `hot-files`, large-file ranking, or `issues` intersection CLI (targeting note comes from the CRAP report).
- No `BuildMode::Coverage` and no `BUILD.yaml` target for analysis.
- Skills are not copied to `.cursor/skills` or `.claude/skills`.
- No CI enforcement of CRAP or coverage thresholds.
- WASM / TypeScript-driven Rust coverage (qape’s minicov path) is out of scope.

### Technical Constraints

- rust-analyzer indexing can exceed a default budget on a loaded machine; `--indexing-budget` is the answer, not a plan defect.
- Binary per-function coverage makes CRAP coarser than a line-weighted variant.
- Duplicate-test comparison is within one coverage run.

## Testing Strategy

- **Unit**: `syn` complexity on fixture sources; CRAP formula; join on declaration line; bitset duplicate/subset.
- **Integration**: coverage collection against a tiny crate with known entered/unentered functions; HTML contains those names; duplicate-tests groups two tests with identical regions.
- **Restructuring**: fixture crate exercises extract_module (including `to_file` + `reexport`), check refusals (name taken, code-bearing plan), dry-run writes nothing, verify against HEAD. Tests that start rust-analyzer run `--test-threads=1` for the suite that binds the server.

## References

### Affected Features (Complete List)

- [Reusable LSP](../reusable-lsp.md) — client/method growth; reuse not replace
- [Feature prompt: agent skills](../feature-prompt-agent-skills.md) — two new valid skills
- [tddy-build](../../build/tddy-build.md) — no v1 contract change; Rust crates remain the analysis/restructure targets

### Related Documentation

- qape-hq: `docs/ft/infrastructure/crap-code-analysis.md`, `docs/ft/infrastructure/rust-test-coverage.md`
- qape-hq: `common/code-analysis`, `common/code-restructuring` (Rust backend + plan schema)
- qape-hq skills: `analyze-code-issues`, `code-restructuring` (source of the tddy imports)
- Complementary: `.cursor/commands/analyze-clean-code.md` (LLM heuristic, unchanged)
