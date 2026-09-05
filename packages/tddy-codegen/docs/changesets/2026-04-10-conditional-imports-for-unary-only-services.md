# 2026-04-10 — Conditional imports for unary-only services

**Type:** Feature

`TddyServiceGenerator` emits `Stream` / `StreamExt` / `mpsc` imports and `_method` only when service has bidi methods; fixes unused-import warnings for unary-only services such as `codex_oauth.CodexOAuthService`. (tddy-codegen)
