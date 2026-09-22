# 2026-09-22 — The `Presenter` impl splits along its own state boundaries

**Type:** Refactor · `#carve` 8/9, PR [#495](https://github.com/uppin/tddy-coder/pull/495)
Cross-package entry: [`docs/dev/changesets/2026-09-22-carve-presenter-split.md`](../../../../docs/dev/changesets/2026-09-22-carve-presenter-split.md)

`presenter/presenter_impl.rs` — one 46-method `impl Presenter` in 1,691 production lines — becomes a
parent plus six child modules, one `impl Presenter` each, along the five state groups from
`presenter/state_groups.rs`: `wiring` (96), `view_channels` (179), `activity` (300), `questions`
(399), `backend_selection` (185), `workflow_run` (469). The parent is 219 production lines: the
struct, the private helpers more than one partition calls, the `poll_workflow` dispatcher and the
three accessors.

`handle_intent` and `poll_workflow` became flat dispatchers; their arms moved verbatim into 24
`pub(super)` handlers in the partition owning the state each touches. That split was forced by
child-module privacy — a private method in a child is visible only there — and budget arithmetic
(790 lines reachable from `handle_intent` against ~699 of room). Every other body is unchanged, and
the 27 previously private methods stay private (`tests/presenter_split_shape.rs`).

Closes `god-object-presenter`. Narrows the `handle_intent` and `poll_workflow` complexity records;
three other complexity records moved with their code. Layout and placement rule:
[architecture.md § Presenter](../architecture.md).
