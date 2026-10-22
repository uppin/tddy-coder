# 2026-10-22 — the production-line count leaves out test-only items (`#reshape` 17/24, #612)

**Type:** Fix

`tddy-code-restructuring` counts a Rust file's production lines as every line except those of each `#[cfg(test)]` /
`#[cfg(all(test, …))]` item, wherever it sits, and exposes it as `production_lines_of_file` and
`tddy-tools restructure lines <file>...` (routed in process by `tddy-tools`, no index daemon). Two shape tests pin
the engine's file budget and the must-not module edges of its planned splits. The five engine file splits were
deferred by the developer (2026-10-21) to the `#reshape` sweep node; `/pr-wrap`'s gate still uses its `awk` count.

Packages: `tddy-code-restructuring`, `tddy-tools`. Detail:
[packages/tddy-code-restructuring/docs/changesets/2026-10-22-oversized-files.md](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-22-oversized-files.md).
