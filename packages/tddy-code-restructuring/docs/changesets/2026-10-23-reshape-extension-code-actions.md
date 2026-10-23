# 2026-10-23 — apply_code_action, and verify hands other languages' changed files to their SourceVerify (`#reshape` 23/24, #682)

**Type:** Feature

`RefactorKind::ApplyCodeAction` with `action: {title} | {kind}` (R4 in the codec through `ActionSelector::wanted`), selection and resolution in `backends/code_action.rs` (R1–R2) for `ServerBackend` and `RustBackend` (`SUPPORTED` 28 → 29). `verify/source.rs` (`SourceVerify`, `SourceFinding`), `Options::verifies`, `runner::{changed_sources, verify_sources, verify_with_sources}` with root-relative paths, `Outcome::VerifiedWithFindings`, `console::{source_findings, verify_refusal}`. `backends/rust.rs` 2,947 → 2,953 production lines (wiring).

Full history: [cross-package entry](../../../../docs/dev/changesets/2026-10-23-reshape-extension-code-actions.md).
