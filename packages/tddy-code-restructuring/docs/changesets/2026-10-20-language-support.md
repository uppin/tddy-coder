# 2026-10-20 — `LanguageSupport`, `SupportRegistry`, `RustSupport`
**Type:** Feature (behaviour-preserving for Rust)

`language.rs` adds the `LanguageSupport` trait (`id`, `claims`, `launch`, `workspace_root -> Result`,
`backend(server, root)`), `ServerHandle`/`LiveServer` and `SupportRegistry` (`with_builtins`,
`with_builtin_rust`, `register`, `support_of`, `allow_list`, `backends`, `backends_static`). First
registration wins for files and for ids. `backends/rust/support.rs` holds `RustSupport`; `registry::Language`
is gone (`LanguageBackend::language() -> &LanguageId`), and `restructure_allow_list` and
`registry_for_static/waiting`'s direct `RustBackend` construction are replaced by the registry. Documents are
opened with the launch's `language_id`. `RustSupport::backend` does not store `root` (the live client carries
its own; a stored root would let the static tier reach a spawn).

Docs: [rust-code-restructuring.md § Languages](../../../../docs/ft/coder/rust-code-restructuring.md#languages).
