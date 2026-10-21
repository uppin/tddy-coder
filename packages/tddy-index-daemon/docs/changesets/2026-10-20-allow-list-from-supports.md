# 2026-10-20 — The allow-list comes from the language supports
**Type:** Refactor (no behaviour change)

`rust_analyzer_as_this_crate_needs_it` is gone: the daemon builds its allow-list with
`SupportRegistry::with_builtin_rust(RustSupport::launching(<configured rust-analyzer>)).allow_list()`, the same
launch the restructure CLI builds, and its `LspKey`s carry `LanguageId::RUST`.
