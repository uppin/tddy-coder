# 2026-10-02 — Cross-crate moves read and rewrite every path the moved file names

`#live-plan` 3/7, PR [#540](https://github.com/uppin/tddy-coder/pull/540). Predecessor:
[#538](https://github.com/uppin/tddy-coder/pull/538). Successors:
[#541](https://github.com/uppin/tddy-coder/pull/541) (`move-facades`),
[#543](https://github.com/uppin/tddy-coder/pull/543) (`check-parity`).

`move_module_to_crate` and `move_cluster_to_crate` used to decide what the moved file depends on from its
header `use` lines read as strings, which left four kinds of tree that did not compile, or refused moves
that were fine. They now take one **path survey** of the file and derive the rewrite, the test for a
dependency back on the origin, and the destination's manifest from it.

- **Headers and bodies.** `use` items at any depth and paths in bodies are surveyed and rewritten the
  same way. `self::` and `super::` (any depth) are resolved by segment against the file's module path,
  then followed through the origin's re-exports, glob and chained, to the crate that defines the item.
- **Rewrite by where a path ends up.** To the destination: `crate::…`, whether written as
  `destination::…` or as a `crate::…` the origin only forwards there. To something staying behind: the
  origin. To a third crate: that crate. A `use` leaf whose name changes keeps the old one with `as`.
- **Edges back are exact.** Only a header or body path to an item the origin itself defines, and which
  stays behind, makes the destination depend on the crate it left. A `super::` into the moved set, an
  import the origin re-exports from the destination, and anything under `#[cfg(test)]` are not edges.
- **Manifest from the survey.** A crate named only in a body is carried; one named only under
  `#[cfg(test)]` goes to `[dev-dependencies]`. The destination is never added to its own manifest, and
  the move asserts it.
- **New refusals.** A `use` group whose members would need different qualifiers ("write one `use` per
  path"), and a module file the walk must read but cannot (any error other than "no such file").

Limits: `check` still reads only the moved file's top-level `use` header, so `check --deep` can pass a
move that `apply` refuses on a body or nested path; `check-parity` moves it onto the survey. Macro
output, `pub(in …)` and `#[path]` modules are not seen. See
[Rust code restructuring](../rust-code-restructuring.md#path-survey).
