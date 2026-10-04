# 2026-10-04 — `Symbols` and `Diagnostics`

**Type:** Feature

`#live-plan` 11/15, PR [#570](https://github.com/uppin/tddy-coder/pull/570). Cross-package entry:
[2026-10-04-session-lsp-tools.md](../../../../docs/dev/changesets/2026-10-04-session-lsp-tools.md).

`code_index.proto` gains `Symbols(SymbolsRequest) → SymbolsResponse` and
`Diagnostics(DiagnosticsRequest) → DiagnosticsResponse`, with `CodeSymbol` and `CodeDiagnostic`
(one-based byte `SourceRange`, `CodeLocation` reused). `src/symbols.rs` serves them on the root's warm
server. `navigation.rs` is refactored, with its behaviour and error messages unchanged: its path check
and document sync become `served_source` and `synced_document`, shared with `symbols.rs`, and
`wire_position` and `code_location` become `pub(crate)`. See
[code-index-service.md](../code-index-service.md).

Tests: `tests/code_index_service_acceptance.rs` — the symbols of a file, the diagnostics of a file, and
each refusing a non-Rust file and a path that climbs out of the root. `Symbols` with a `query` is not
covered at this level: the shared fake language server answers only `textDocument/documentSymbol`.
