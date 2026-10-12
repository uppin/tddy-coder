# 2026-10-09 — `edits_for_file`'s per-file state was grouped into a struct by hand

**Category:** Technical debt (hand design edit made with consent before engine extractions)
**Source:** #reshape 19/19 (`fn-sizes-backend`), decision F2 (consented by the developer, 2026-10-09).

`backends/rust/item_move/sites.rs` `edits_for_file` (91 lines on 2026-10-09) derives `masked`, `statements`, `base`,
`qualifier` and two closures (`in_region`, `in_destination`) from its four parameters, then walks two loops that read
all of them. Every loop-level `extract_method` range either reads a closure, which rust-analyzer cannot name and the
engine's inferred-placeholder check refuses, or takes 8 or more parameters (`clippy::too_many_arguments`). Engine-only
cuts reach about 65 lines.

With consent, the derived values and the two closures were gathered into one private `FileSites` struct with
`in_region` and `in_destination` methods, by hand, marked `TODO(reshape-19)`. The two loops were then cut by
`extract_method`. Record here the plan names and the commit of the hand edit when it lands.

## Why deferred

The engine has no operation that introduces a parameter struct or turns a closure into a method. The missing operation
is `2026-10-09-restructure-has-no-operation-to-introduce-a-parameter-struct.md`, and this is its second instance after
`apply_held_plan`. When that operation exists, this edit is the shape it should reproduce. Remove the `TODO(reshape-19)`
marker and this entry once an engine run could have produced the same struct.
