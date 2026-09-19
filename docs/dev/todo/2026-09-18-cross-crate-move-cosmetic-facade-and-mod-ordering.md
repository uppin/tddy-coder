# 2026-09-18 — `move_module_to_crate` writes a facade per operation and appends `pub mod` out of order

**Category:** Cosmetic — output quality
**Source:** re-filed by `#carve` node 3, [#490](https://github.com/uppin/tddy-coder/pull/490), out of
[`2026-09-09-restructure-defects-from-the-first-cross-crate-move.md`](./2026-09-09-restructure-defects-from-the-first-cross-crate-move.md),
which that node fixes completely and whose wrap therefore deletes it. First observed on `#unbundle`
node 1, [#470](https://github.com/uppin/tddy-coder/pull/470).

**These two are unfixed.** They are recorded here so they do not leave with the file that happened to
record them — the rest of that entry is resolved, these two are not.

- **One `pub use <crate>::*;` per operation rather than one per destination.** A ten-operation plan
  moving ten modules to one destination appends **ten identical lines** to the origin's `lib.rs`.
  Correct output is one line per destination crate, however many modules travelled to it.
- **`pub mod` lines are appended after whatever the destination's `lib.rs` already said**, rather than
  in order. The result compiles and reads as unsorted, which every later reader has to either
  tolerate or fix by hand.

## Why they were left

Both are output-shape defects in a tree that compiles and passes its tests, so neither blocks a move.
`#carve` 3/9's `## Boundaries` name them as explicitly out of scope: it widens the operation to move a
**cluster**, and folding a cosmetic rewrite of the facade and manifest writers into that diff would
mix a behavioural change with a formatting one in the file under most change.

Both are more visible after that node, not less: a cluster move is by definition several modules
going to one destination, which is exactly the shape that produces the duplicate facade lines.

## Where the code is

After `#carve` 3/9's Phase A split, both writers live in
`packages/tddy-code-restructuring/src/crate_move/`:

- the facade line — `facade_line` in `crate_move.rs`, called per operation
- the `pub mod` append — `after_last_module_declaration` in `manifest_edits.rs`
