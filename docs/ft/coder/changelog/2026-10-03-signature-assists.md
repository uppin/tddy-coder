# 2026-10-03 — Operations that remove a parameter or name a tuple return, callers rewritten

`#live-plan` 9/15, PR [#569](https://github.com/uppin/tddy-coder/pull/569). Feature:
[Rust code restructuring](../rust-code-restructuring.md).

Two signature changes rust-analyzer makes together with every caller are now plan operations. Each
leaves a compiling tree on its own, so neither needs a transactional group.

- **`remove_unused_param`** drops a parameter from the declaration and from every call site, in every
  file. `name` is the parameter. A parameter the function still reads is refused, naming it and saying
  it is used; nothing is written.
- **`convert_tuple_return_to_struct`** turns `-> (A, B)` into `-> Name`, a new tuple struct that keeps
  the function's visibility, and rewrites destructuring callers to `let Name(a, b) = …`. `name` is the
  struct.
- A plan that gives either operation no `name` is refused as malformed before any language server
  starts.

Operations that break callers — changing a return type to another type, adding, reordering or retyping
a parameter — are not covered; see the known limitations.
