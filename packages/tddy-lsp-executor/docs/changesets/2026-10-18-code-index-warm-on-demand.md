# 2026-10-18 — Lsp* answers say the index is still loading
**Type:** Feature

A definition, references or hover answer the index gives while `LOADING` keeps the tool's own key and
adds `"index": "loading"` with a `note` — "this empty answer is not final" when empty, "this answer may be
incomplete" otherwise. `is_available` uses `tddy_index_daemon::is_rust_workspace`. Tests in
`lsp_tools_via_index.rs`.

Docs: [index-backed-executor.md](../index-backed-executor.md#what-the-tools-see). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).
