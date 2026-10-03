# 2026-10-03 — the stranded-sibling finding reads only the top-level `use` header

**Category:** Future enhancement (check/apply parity for `move_cluster_to_crate`)
**Source:** `#live-plan` 6/7, [#543](https://github.com/uppin/tddy-coder/pull/543), found while
resolving the `TODO(check-parity)` marker that `move-paths` (3/7) left in `crate_move/header.rs`.

## What is left

`cluster::paths_naming_the_origin` (the stranded-sibling finding) returns
`RepointedHeader::header_origin_paths` — only the paths written in the moved file's top-level `use`
header. `apply`'s cycle refusal (`refusals::refuse_a_dependency_cycle`) reads **every** path, bodies
and nested `use` items included. So for a cluster the two can disagree: `check` can pass a sibling
that `apply` then refuses.

#543 closed the **body-path** half for a single move (`stays_behind_through_a_body`), which is a
different finding with its own wording. It did not touch this one.

## Why it was left

Switching the reader to `origin_paths` (and deleting `header_origin_paths`) widens a finding that has
its own tests and message text, so it is a behaviour change to be made on purpose, not a field
cleanup — and it is outside #543's `## Boundaries` (`check`'s two new findings only).

## What would close it

Widen the stranded-sibling finding to `origin_paths` with its own tests and wording, then delete
`header_origin_paths` and the `TODO(check-parity-header)` marker in `crate_move/header.rs`.
