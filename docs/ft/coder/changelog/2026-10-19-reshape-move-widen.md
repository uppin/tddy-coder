# 2026-10-19 — a cross-crate move widens what the origin still reaches, and says so

- **Reached declarations land `pub`.** `move_module_to_crate` and `move_cluster_to_crate` write `pub` over
  every item, inline-module item and its `mod` declarations, child `mod` declaration, struct field and
  inherent method or const of a moved file that a file staying behind, outside the destination crate,
  still references. Nothing else is widened; a reference from a co-moving member or from inside the
  destination widens nothing, and trait-impl members, trait items, enum variants and anything already
  `pub` are never touched.
- **A parent's glob counts.** Every non-private item of a module its parent re-exports with `use
  <module>::*;` is widened, with the glob as the reason, and so is every type a widened signature names.
- **The widening is reported before it is written.** `apply` prints each one as `visibility: …` with its
  reason, the journal records it, and `check --deep` prints the same lines; its survey line names the glob
  that explains a module with reached items and no callers.
- **A keyword on a line above its name is refused**, naming the declaration, with nothing written.
- **Not widened:** tuple-struct fields, `macro_rules!` items, in-file `use` re-exports, associated types of
  an inherent `impl`, and an item only a macro expansion in the origin names; the compile gate names them.
