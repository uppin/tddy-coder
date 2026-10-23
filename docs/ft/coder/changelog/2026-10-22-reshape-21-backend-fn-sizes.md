# 2026-10-22 — the engine's backend is held to 60-line functions; more `extract_method` limits are documented

`#reshape` 21/24. The function-length test of `tddy-code-restructuring` now covers `src/backends/` too. Eight backend
functions were cut with `extract_method` itself; six the engine could not yet cut are exempted by name, and the list only
shrinks. Cutting them showed further limits of `extract_method` and `verify`, now listed in
[Rust code restructuring § Known limitations](../rust-code-restructuring.md#known-limitations).
No operation, plan field, flag or output line changes.
