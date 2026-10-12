# The Rust backend fits the 500-line budget: `backends/rust.rs` keeps the type, its modules hold the rest - PRD

**Date**: 2026-10-09
**PRD Type**: Behaviour-preserving restructure (the engine's own oversized file, split by the engine)
**Stack**: `#reshape` 17/19 (`feature/reshape/rust-backend-split`, base `feature/reshape/fn-sizes-rest`; real parents
`feature/reshape/move-impl-members`, `feature/reshape/multi-seam-extract`, `feature/reshape/tidy-facades`, and
`feature/reshape/widen-same-crate` and `feature/reshape/oversized-files`)

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md). No section changes. No operation, plan
  field, flag, output line or wire message changes. The public paths `tddy_code_restructuring::backends::rust::*` and
  `tddy_code_restructuring::{client_capabilities, server_settings}` keep resolving.

The documents that change are package documents. They are updated at wrap, because they name `backends/rust.rs` as the
place that dispatches, waits or rewrites:
- `README.md` (the source layout table and the over-budget sentence);
- `docs/assist-output-repairs.md`, `docs/readiness-and-gates.md`, `docs/signature-rewrites.md`,
  `docs/signature-assists.md`, `docs/same-crate-moves.md`, `docs/retarget-impl.md`, `docs/repoint-call.md` and
  `docs/repoint-facade.md`.

## Summary

`backends/rust.rs` goes from about 2,850 production lines at this node's base (2,958 on master) to about 425. Every move is
made by `tddy-tools restructure`:
- the members of `impl RustBackend` by `move_impl_members`;
- the trait impls by `move_item`;
- the free items by `move_item`.

They land in nine new child modules of `backends::rust` and six existing ones, one module per responsibility. What stays in
`rust.rs` is the type, its constructors, the shared refusal and identifier helpers, and the `mod` list. No behaviour
changes. The shape tests from `#reshape` 15 lose their `rust.rs` exemption and gain the module edges that must not exist.

## Background

`rust.rs` has been over budget in every PR of three stacks. Its code-issue record has fourteen rows, and each is wiring
that one more operation added. `#live-plan` 7/15 moved nine runs of free helpers out and stopped at the members of
`impl RustBackend`, because no operation could move a member into another module. `#reshape` 13 adds that operation.

What is left is not only members. The leftovers todo and the code-issue record both say "the free-item runs are done".
The code disagrees: about 790 lines of free items remain. They include the assist catalogue (`assist_for` alone is 93
lines), the handshake, LSP geometry, edit conversion and the refusal builders of the assist offer. Node 13's own discovery
says its operation alone cannot reach 500. So this node uses three operations, each where it fits:
- `move_impl_members` for the 17 member runs;
- `move_item` for the three trait impls, as one run;
- `move_item` for 30 free-item runs.

`extract_module` is not used (F1). It can only create a new child module, while six groups join existing modules. It
widens everything to `pub(crate)`. It needs one contiguous range, while the free items of one group sit in up to six
places. It leaves facade lines in `rust.rs`, which count against the budget. `move_item` has none of those limits.

## Proposed Changes

### What's Changing

- **The layout.** All modules are children of `backends::rust`. Sizes are projected production lines after the split, at
  this node's base.

  | Module | Status | Holds | Lines |
  |---|---|---|---:|
  | `rust.rs` | stays | the type, its constructors, `ProgressSink`/`discard`, the shared refusal and identifier helpers, the `mod`/`use` hub, the public re-exports | ~425 |
  | `language_backend.rs` | new | `SUPPORTED`, `impl Drop`/`LanguageBackend`/`ModuleReferences`, `resolve_opening`, `outside_references_opening` | ~385 |
  | `assists.rs` | new | the assist catalogue (`Assist`, `Placeholder`, `assist_for`) and the offer (`assist`, `offered_assist`, its refusals) | ~365 |
  | `transport.rs` | new | JSON-RPC: `start`, `request`, `request_settled`, `notify`, `send`, `receive`, `did_change` | ~310 |
  | `assist_edits.rs` | new | assist results as workspace edits: `edit_for`, `chain_module_to_file`, `multi_file_assist`, `convert_change` | ~300 |
  | `extraction.rs` | new | `assisted_edit`, `extract`, `rename_placeholder`, the assist-import pruning | ~255 |
  | `seam_reach.rs` | new | the seam survey's server queries: `survey_moved_items`, `survey_impl_members`, `reach_of` | ~190 |
  | `references.rs` | new | `references_outside`, `references_at`, the symbol walk `path_reached_within` | ~180 |
  | `symbols.rs` | new | `anchor_range`, `locate_symbol`, `rename_symbol` | ~145 |
  | `handshake.rs` | new | `client_capabilities`, `server_settings`, the encoding negotiation (no `RustBackend`) | ~110 |
  | `readiness.rs` | existing, 303 | + `keep_waiting`, `beat`, `waited_on`, `incomplete_index` | ~400 |
  | `lsp_edits.rs` | existing, 247 | + `LspPoint`, `LspEdit`, `uri_of`, `path_of`, `relative_to` | ~305 |
  | `documents.rs` | existing, 59 | + `settled_outline` and its emptiness test | ~120 |
  | `signature_rewrites.rs` | existing, 175 | + `rewrite_signature` | ~205 |
  | `return_type.rs` | existing, 46 | + `wrap_or_unwrap_return_type` | ~100 |
  | `imports.rs` | existing, 397 | + `IMPORT_PASSES` | ~411 |

- **Visibility.** A moved private member or item that its callers need becomes `pub(super)`, which is as wide as a child
  of `backends::rust` needs. Fields of a moved struct that other modules read are widened to `pub(super)`. Nothing becomes
  `pub(crate)` or `pub` that was not already.
- **Callers.** Callers inside the crate are re-pointed (`reexport: none`). The two `pub` functions that other packages
  reach (`client_capabilities`, `server_settings`) move with `reexport: outside`. `lib.rs` is re-pointed, the outside
  callers are not edited, and one `pub use` line keeps `backends::rust::client_capabilities` public as today.
- **Shape tests.** `tests/engine_file_budget_shape.rs` holds every production file of the crate to 500 lines with no
  exemption. `tests/engine_module_edges_shape.rs` adds five checks:
  - `rust.rs` holds one inherent `impl` and no trait impl;
  - the session modules reach no operation module;
  - `handshake` and `lsp_edits` never name the backend type;
  - nothing imports the dispatcher;
  - the assist catalogue reaches none of its users.

### What's Staying the Same

- Every operation's behaviour, refusal text, report line and note.
- Function bodies. No function on the >60-line list grows. Node 19 shortens them afterwards in their new files.
- The inline tests in `rust.rs` (about 2,560 lines). They are not production lines, and no operation splits an inline
  test module (proposed todo).
- The public paths listed under Affected Features. No file outside `tddy-code-restructuring` is edited.
- Moves are engine-driven only. A hand edit fixes the build after an engine move and nothing else, and each one gets a
  `docs/dev/todo/` entry. A refusal stops the run, which is rolled back, and the developer is asked. Never `git mv`.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring` only:
  - nine new files;
  - six files grow;
  - about 30 child files have a `use super::…` re-pointed;
  - the inline tests gain `use` lines;
  - the two shape tests change.
- After this node, `backends/rust/` holds 20 modules with an `impl RustBackend` block, up from 10. That is the house
  pattern (`item_move.rs:53`, `readiness.rs:42`, …), and it is the seam node 18 turns into free functions over a session
  handle.
- Watch list after the split: `imports.rs` (~411), `readiness.rs` (~400, more if node 10's bound logic lands there),
  `language_backend.rs` (~385).

### User Impact

None. No command, plan line or output changes. Engine developers find each responsibility of the Rust backend in a file
named for it.

## Implementation Plan

1. Baseline: `./test -p tddy-code-restructuring -p tddy-index-daemon`, with failing tests recorded by name. Re-measure
   `rust.rs` and every destination with `restructure lines`.
2. Edit the shape tests (red).
3. Eleven plans, one per destination group. For each: `restructure anchors`, then `check --deep`, then `apply --dry-run`,
   then `apply`, then `verify --against HEAD`, then a commit.
4. Final gate: fmt, clippy `-D warnings`, the baseline by name, `verify` over the whole run, the comment-line multiset,
   and the shape tests green.
5. Wrap: the package docs; delete the code-issue record and the two claimed todos, with the final measurement recorded;
   remove leftovers § 1.

## Acceptance Criteria

- [ ] every production file of `tddy-code-restructuring/src`, `backends/rust.rs` included, is at most 500 lines by `restructure lines`
- [ ] `backends/rust.rs` holds exactly one `impl RustBackend` (the constructors) and no `impl … for RustBackend`
- [ ] `handshake.rs`, `transport.rs`, `readiness.rs`, `documents.rs`, `references.rs` and `lsp_edits.rs` name no operation module
- [ ] `handshake.rs` and `lsp_edits.rs` never name `RustBackend`
- [ ] no module of `backends/rust/` imports `language_backend`
- [ ] `assists.rs` names none of `extraction`, `assist_edits`, `symbols`, `seam_reach`, `language_backend`
- [ ] the public paths of `backends::rust` and the crate root resolve unchanged; `tddy-index-daemon` builds and its tests compile with no edit
- [ ] every move was made by `tddy-tools restructure`; each hand fix has a todo entry
- [ ] `restructure verify --against <ref before plan A>` holds, and the multiset of comment lines is unchanged
- [ ] tests pass for `tddy-code-restructuring` and `tddy-index-daemon`, with the failing set equal to the baseline's by name (scoped; CI for the rest)

## Decisions

F1–F10 (changeset § Decisions & Trade-offs) ✅ decided 2026-10-09: every recommendation taken.

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md) (no section changes)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-rust-backend-split.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-rust-backend-split-initial-discovery.md`
- Records closed here:
  - [oversized-file-backends-rust.md](../../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md);
  - [2026-09-16 backends/rust.rs is 4,500 production lines](../../../dev/todo/2026-09-16-backends-rust-rs-is-4500-production-lines.md);
  - [2026-10-03 the Rust backend grows with every live-plan node](../../../dev/todo/2026-10-03-restructure-rust-backend-grows-with-every-live-plan-node.md);
  - [leftovers of the live-plan carve](../../../dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md), § 1.
- Successors: `feature/reshape/backend-session` (operations become free functions over a session handle) and
  `feature/reshape/fn-sizes-backend` (the eight long functions moved here are shortened there).
