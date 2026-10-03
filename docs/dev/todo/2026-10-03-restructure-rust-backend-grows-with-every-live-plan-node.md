# 2026-10-03 — `backends/rust.rs` keeps growing through the `#live-plan` stack

**Category:** Deferred from `#live-plan` 9/15 (#569) — file-length gate
**Source:** `/pr-wrap` step 3.5; standing record
`packages/tddy-code-restructuring/docs/code-issues/oversized-file-backends-rust.md`

#569 grew `packages/tddy-code-restructuring/src/backends/rust.rs` from 2,666 to 2,705 production lines
(budget 500): two `SUPPORTED` entries, two `assist_for` rows and the dispatch in `multi_file_assist`. The
signature assists' own logic went into `backends/rust/signature.rs` (403 lines) instead.

**Why the split is deferred, with the developer's consent (2026-10-03):** every other open `#live-plan`
node — `transactional-groups`, `session-lsp-tools`, `indexing-indicators`, `plan-dialog`,
`session-restructure-tools` and `signature-rewrites` — also edits this file, so splitting it inside the
stack would turn each of their diffs into a conflict. The decomposition belongs on a follow-up branch
**after the stack lands**; the standing record carries the measurement history and the seams.

`plan.rs` (387) and `plan/codec.rs` (393) are under budget.
