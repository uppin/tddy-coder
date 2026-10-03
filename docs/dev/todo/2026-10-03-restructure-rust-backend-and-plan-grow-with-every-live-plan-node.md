# 2026-10-03 — `backends/rust.rs` and `plan.rs` keep growing through the `#live-plan` stack

**Category:** Deferred from `#live-plan` 9/15 (#569) — file-length gate
**Source:** `docs/dev/1-WIP/2026-10-03-restructure-signature-assists.md`, `/pr-wrap` step 3.5

#569 grew two files already past the 500-production-line budget:

| File | Production lines | This PR |
|---|---|---|
| `packages/tddy-code-restructuring/src/backends/rust.rs` | 4483 → 4522 | +39 (the dispatch in `multi_file_assist`, two `SUPPORTED` entries, two `assist_for` arms) |
| `packages/tddy-code-restructuring/src/plan.rs` | 936 → 964 | +28 (two `RefactorKind` variants and the `parse_op` `name` refusal) |

The signature assists' own logic went into `backends/rust/signature.rs` (402 lines) rather than
`rust.rs`; that is all this node did to contain the growth.

**Why the split is deferred, with the developer's consent (2026-10-03):** every open `#live-plan`
node, parents and dependents, edits both files. Splitting either inside the stack turns each of those
diffs into a conflict. The decompositions belong on a follow-up branch **after the `#live-plan` stack
lands**; the seams are described in the `oversized-file-backends-rust` and `oversized-file-plan`
records under `packages/tddy-code-restructuring/docs/code-issues/`, which carry the regression rows.
