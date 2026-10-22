# 2026-10-22 — a gate fails on any engine function past 60 lines outside the backend; the limits `extract_method` showed are documented

`#reshape` 18/24. `tddy-code-restructuring` has a test that fails on any production function longer than 60 lines
outside `src/backends/`, and five of the functions it found were cut with `extract_method` itself. Ten functions the
engine could not yet cut (or that the stack shares with other work) are exempted by name, and the gate fails when
an exempted function drops to 60 or fewer so the list only shrinks. Cutting them showed four limits of `extract_method`
and of `verify`, now listed in [Rust code restructuring § Known limitations](../rust-code-restructuring.md#known-limitations).
No operation, plan field, flag or output line changes.
