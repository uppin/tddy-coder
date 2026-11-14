# 2026-11-14 — The `#orchestrators` sweep: `lib.rs` split into shell, search and LSP tools

**Type:** Refactor

`#orchestrators` 12/12, PR [#901](https://github.com/uppin/tddy-coder/pull/901).

## What changed

- **B5** — the shell surface moved to `shell_tool`, Grep and Glob to `search_tools`, and the LSP
  bridge to `lsp_tools`, each kept reachable with `reexport: "outside"`. `lib.rs` is 475
  production lines (re-measured at wrap to the first `#[cfg(test)]` module).

## Code issues

- `oversized-file-lib.md` deleted: 893 → 475 production lines (final measurement above).
