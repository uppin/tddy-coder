# 2026-10-21 — `WorkspaceGraph::crates_building`

**Type:** Feature

`#satellite` 14/15 ([#689](https://github.com/uppin/tddy-coder/pull/689)).

`WorkspaceGraph` reads each member's `bin` targets from the same `cargo metadata` call as its
dependencies. `crates_building(binary)` answers which workspace crates build a binary, so a boundary
suite can hold that a binary is built by a wiring crate and not by a library of the same name.
