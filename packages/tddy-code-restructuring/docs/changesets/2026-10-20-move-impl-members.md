# 2026-10-20 — `move_impl_members`
**Type:** Feature

`RefactorKind::MoveImplMembers` with its codec rules (`plan/codec/impl_move_fields.rs`), plain-`check` findings
and a live binding in `backends/rust/impl_move/` (`preflight`, `survey`, `survey/stayed`, `cut`, `landing`,
`assemble`). `check` and `resolve_opening` fold the three same-crate kinds into one arm each
(`same_crate_dispatch.rs`), so `backends/rust.rs` is 6 production lines shorter (3,041 to 3,035) with the new
kind registered. `retarget_impl::outline::{read, Run}` are visible to the backend and take the operation's name
for their S1/S2 wording; `item_move::members::MemberWidening` gained `other_files`, which `item_move` never sets;
`item_move/assemble.rs` has the reached-item widening extracted to `widen_reached_items` by the engine
(509 to 519 production lines). `.config/rust-e2e.filterset` and `nextest.toml` register the live binary.

Resolved from the backlog: `docs/dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md`
§ 1 is narrowed to the runs not yet being moved. Added to it:
`2026-10-20-restructure-move-impl-members-skips-impls-in-an-ancestors-inline-module` and
`2026-10-20-restructure-reached-item-widening-is-written-twice`.

Docs: [impl-move.md](../impl-move.md),
[rust-code-restructuring.md § `move_impl_members`](../../../../docs/ft/coder/rust-code-restructuring.md#move_impl_members).
