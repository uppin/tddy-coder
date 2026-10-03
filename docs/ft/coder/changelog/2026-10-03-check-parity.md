# 2026-10-03 — `check --deep` refuses the cross-crate moves `apply` cannot build

`#live-plan` 6/7, PR [#543](https://github.com/uppin/tddy-coder/pull/543). Feature:
[Rust code restructuring](../rust-code-restructuring.md).

Two cases where a clean `check --deep` was followed by a move `apply` could not build are now reported
by `check` itself, before any index is paid for.

- **A module's body reaches a module that stays behind.** `crate::host::f(…)` in a function body, with
  `host` remaining in the origin, is reported with the file, the path and the line. The remedy is to cut
  the dependency; a cluster is never suggested, because the host is not a sibling that can come along.
  A module an earlier operation of the same plan already moved to the same destination is not reported.
- **The destination already has the module.** A root that declares the name, or a file already at the
  target path, is reported as a merge, which no operation performs.

A `use` nested in a function, and the stranded-sibling finding of a cluster, still read the top-level
`use` header only; see the known limitations.
