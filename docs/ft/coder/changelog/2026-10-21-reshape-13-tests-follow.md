# 2026-10-21 — Cross-crate moves take the test modules that test only the moved code

`#reshape` 13/24. `move_module_to_crate` and `move_cluster_to_crate` take a sibling `#[cfg(test)] mod t;`
declared beside a moved module when everything it names in the origin moves: its files land beside the module,
its declaration (with its doc comments) goes to the destination root, its paths are re-pointed and its crates
join `[dev-dependencies]`. A test module that also needs code staying behind stays, and the resolution says
which path keeps it. A file reached through a `#[cfg(test)] mod` is test code throughout, and a path through the
parent's re-export of a co-moving module is re-pointed into the destination. A landing that already exists is a
merge, refused by `apply` and reported by `check` in the same words. See
[Rust code restructuring](../rust-code-restructuring.md#known-limitations).
