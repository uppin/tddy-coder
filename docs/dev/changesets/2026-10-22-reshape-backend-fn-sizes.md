# 2026-10-22 — the Rust backend's functions past 60 lines are cut or recorded; the gate covers `src/backends/` (`#reshape` 21/24, #680)

**Type:** Refactor

`tddy-code-restructuring`'s function-length gate (`tests/function_length_budget.rs`) now also fails on any production
function under `src/backends/` longer than 60 lines. Eight of the fourteen it found were cut to 60 or fewer with the crate's own
`extract_method` (19 cuts, one consented hand edit for clippy's `let_and_return`). The developer deferred the other six on
2026-10-22 and they are exempted by name: `choose_import`, `survey`, `next_import`, `request`, `assisted_edit`,
`edits_for_file`; each names an engine defect `extract_method` showed. No behaviour change.

Backlog entries this change added to or left standing, owned by the `#reshape` sweep node:
`2026-10-22-extract-method-respells-dyn-lifetime-and-clone-as-to-owned` (§§ 7–13),
`2026-10-22-verify-does-not-account-for-mut-param-writes-from-extract-method`,
`2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct`,
`2026-10-06-restructure-item-move-assemble-past-500`.

Packages: `tddy-code-restructuring`. Detail:
[packages/tddy-code-restructuring/docs/changesets/2026-10-22-backend-fn-sizes.md](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-22-backend-fn-sizes.md).
