# 2026-10-02 — Cross-crate moves leave one named facade per destination

`#live-plan` 4/7, PR [#541](https://github.com/uppin/tddy-coder/pull/541). Predecessor:
[#540](https://github.com/uppin/tddy-coder/pull/540) (`move-paths`).

After the paths are right, a move could still leave a tree that did not build or lint. It now writes
one grouped `pub use <dest>::{a, b};` per destination for the whole plan instead of a root glob per
operation, so no origin name is shadowed and no duplicate line is written. A module added to the
destination's root is declared in sorted position. A nested module's parent `use <module>::*;` and
`use <module>::{…};` are rewritten to the destination when no facade is left. A test-binary move after
a module move names the crate that defines the module, seeing through a crate-root facade.

A `crate::` path in the moved file's `mod tests` is re-pointed at `origin`, which becomes a
dev-dependency of the destination, and the destination's tests build. The first plan was to refuse
that move; `move-paths` already reads a path under `#[cfg(test)]` as never an edge back, and a
dev-dependency cycle is legal.

Not done: counting the items a parent glob re-exports as reached from outside the crate — a glob
names none. Backlog: the glob-facade shadowing, nested-parent glob, `mod tests` `use` lines and
facade/`pub mod` ordering entries are closed by this change; the nested-parent and test-binary
entries stay, narrowed to what remains.
