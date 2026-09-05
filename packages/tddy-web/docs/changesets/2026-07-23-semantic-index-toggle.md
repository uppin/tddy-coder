# 2026-07-23 — Semantic index toggle

**Type:** Feature

the managed-codebase section (claude-cli + cursor-cli) gains a "Semantic index" checkbox (`create-session-semantic-index-toggle`); `StartSession` sends `semantic_index` (true only when checked, reset when Managed codebase is off). Regenerated `connection_pb.ts` for the new field. Cypress `CreateSessionSemanticIndex` (5). Feature [semantic-index.md](../../../../docs/ft/coder/semantic-index.md). (tddy-web)
