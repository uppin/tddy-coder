# 2026-10-23 — ExtensionVerify, code-action kinds, and a testkit feature (`#reshape` 23/24, #682)

**Type:** Feature

`ExtensionVerify`: new error diagnostics read by pull (closed on every path) and `ERROR` / `MISSING` parse nodes. `ExtensionSupport::{parse, document_language_id, with_code_action_kinds}`, `InstalledExtensions::register_installed`. Feature `testkit` (`install_demo_toml`, `declare_in_demo_toml`, `commit_baseline`). `src/support.rs` 491 → 536 production lines, not split (stack-shared with #683): `docs/code-issues/oversized-file-support.md`.

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
