# 2026-08-31 — Rust code analysis and restructuring on tddy-tools

- **`tddy-tools analyze`** adds llvm-cov capture, CRAP scoring (join on `(file, declaration line)`), HTML report, and duplicate/subset test detection for Rust crates. Library: `tddy-code-analysis`. Skill: `analyze-code-issues`.
- **`tddy-tools restructure`** replays JSONL intent plans through rust-analyzer via `tddy-lsp` (`apply`, `status`, `check`, `anchors`, `verify`). Library: `tddy-code-restructuring`. Skill: `code-restructuring`. Seven Rust operations; plans refuse code-bearing fields.
- **No new binaries** — both surfaces live on the existing `tddy-tools` binary. **Rust only** in v1 (no TypeScript analysis or restructuring).
- **`tddy-lsp`** exposes `request_raw` / `notify_raw` for the restructuring bridge; typed assist APIs remain a follow-up.
- ⚠️ End-to-end coverage capture on a fixture crate is not yet in CI; unit tests cover CRAP join and duplicate bitset logic.
