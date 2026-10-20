# 2026-10-20 — Languages are supports behind one interface; Rust is the first

`#reshape` 11/24. A language is now a `LanguageSupport` — its files, its server launch, its workspace-root
rule and its backend in one place — held by a `SupportRegistry` that builds both the server allow-list and
the backend registry. Built-in Rust is the only support and behaves as before; the CLI and the warm index
daemon now build the same rust-analyzer launch from it. A support registered later never takes a file or an
id a built-in already has. Errors name the backend by id (`rust`). See
[Languages](../rust-code-restructuring.md#languages).
