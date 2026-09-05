# 2026-07-22 — Reusable language server (LSP) via one MCP interface

- Agents get real language intelligence: a reusable language server (rust-analyzer first; any LSP pluggable, chosen by build-target type and gated by an allow-list) runs as a long-running task, reused across build targets, keyed by (workspace root, language) ([reusable-lsp.md](../reusable-lsp.md)).
- A single, language-agnostic MCP tool set — `LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover`, `LspSymbols` — is exposed only when a server is available for the repo (`TDDY_LSP_TOOLS` gate).
- The workspace-level `ReadLints` tool is now backed by real diagnostics (`workspace/diagnostic` pull) when a server is available, else the previous no-linter stub.
- Servers are lazily started, reused across targets, torn down when idle, and re-spawned after a crash; owned by the daemon and sandbox-app.
