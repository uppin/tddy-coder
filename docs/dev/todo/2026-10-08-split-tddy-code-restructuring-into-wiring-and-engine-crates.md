# 2026-10-08 — Split `tddy-code-restructuring` into engine crates and leave it as a wiring package

**Category:** Future enhancement (crate size)
**Source:** #carve 21/21 (PR #536) — LoC survey of `packages/`, 2026-10-08

`tddy-code-restructuring` is ~71.8k lines (52.4k in `src/`, 19.4k in `tests/`) and ~35.4k of
**production** code once inline `#[cfg(test)]` modules are excluded (brace-depth strip, ±5 %). That
is 3.5× the 10k production-line target the `#carve` stack holds every crate to. The developer's
direction (2026-10-08): leave `tddy-code-restructuring` as **only a wiring package**.

## Proposed split (bottom to top; line counts approximate)

| # | Crate | Contents | Prod lines | Depends on |
|---|---|---|---:|---|
| 1 | `tddy-restructure-model` | `edit`, `overlay`, `apply`, `plan`, `plan_store`, `ledger`, `journal`, `item_anchor`, `spawn_record`, `registry`, `verify` | ~7.1k | none |
| 2 | `tddy-restructure-crate-move` | `crate_move/`, `crate_move.rs` | ~5.2k | 1 |
| 3 | `tddy-restructure-rust-support` | shared Rust helpers (`item_path`, `imports`, `import_text`, `module_text`, `chatter`, `selection`, `readiness`, `wait`, `server_process`, `line_diff`, `visibility`, `prelude_shadow`, `introduced`, `documents`, `return_type`), `lsp_bridge` | ~4.5k | 1, 2 |
| 4 | `tddy-restructure-rust-moves` | `item_move/` (the hub), `early_return`, `module_reparent`, `nested_modules`, `seam_survey`, `seam_refusal` | ~6.5k | 1–3 |
| 5 | `tddy-restructure-rust` | `repoint_call`, `repoint_facade`, `retarget_impl`, `signature*`, the `backends/rust.rs` dispatcher | ~5.9k | 1–4 |
| 6 | `tddy-restructure-runner` | `runner/`, `runner.rs`, `console` | ~5.1k | 1, 2; 5 through a trait |
| 7 | `tddy-code-restructuring` | `lib`, `restructure_args`, `restructure_cli`, `pub use` facade | ~0.9k | all |

Tests follow their code. `tests/harness/` (2.7k) is shared by most acceptance suites, so it becomes
a `tddy-restructure-testkit` dev-dependency crate; the CLI and parity suites stay in the wiring crate.

## Cycles to cut before any move

1. `crate_move` → `registry`/`plan`, while `registry` (1 reference) and `item_anchor` (2) point back
   to `crate_move`. Probably shared types; they move down into the model crate.
2. `apply` → `spawn_record` → `backends` → `apply`. The `spawn_record` → `backends` edge is one
   reference.
3. `plan` ↔ `plan_store` and `ledger` → `plan`: single references, absorbed by crate 1.

## Open questions

- **Runner and the Rust backend.** `runner` names `backends` 11 times. Recommended: `runner` takes the
  backend through a trait defined in the model crate and the wiring crate picks the Rust backend. If
  those references are to concrete types, that is the first thing to look at.
- **Is `backends/rust.rs` a dispatcher into every operation?** Not verified. If the operations call
  back into it, it belongs in crate 3, not 5.
- **`tddy-restructure-testkit`** for `tests/harness/`: acceptable?

## Why this was deferred

Found while surveying crate sizes during #536, which is itself a move node on a different crate. This
is its own changeset (edge table, cycle cuts, per-crate baselines) and should be planned after the
`#carve` moves land, because the cross-crate moves it needs run through the same engine whose
refusals #536 is filing todos for (restricted `mod` declarations, grouped `use` lines, directory
children). Moving the engine with itself is likely to need hand fixes, each with a todo.

Public paths under `tddy_code_restructuring::…` stay reachable through `pub use`; no consumer is edited.
