# 2026-10-23 — the warm client renders verify findings (`#reshape` 23/24, #682)

**Type:** Feature

`index_console` renders `VerifyResponse.source_findings` and refuses through `console::verify_refusal`; `ColdLanguages { supports, verifies }` replaces the tuple the cold path took.

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
