# 2026-10-06 — A restructure `move_item` writes what a reader would have written

Feature: [Rust code restructuring](../rust-code-restructuring.md#same-crate-moves).

`move_item` writes three things the way a hand edit would, so a moved item reads the same after a plan
as after a person moved it:

- **A caller keeps its short form.** A caller that reached the item through a one-segment module
  qualifier bound by a `use` in its scope (`use crate::pairing; … pairing::f(..)`) gets the
  destination's last segment bound with a new `use` and the qualifier rewritten to it, instead of the
  full `crate::answers::f(..)` inlined. The old `use` is pruned by the end-of-run tidy when nothing
  else needs it. If the destination's last segment is already taken in the caller's scope, today's full
  path is written and the run says so.
- **A facade path in the moved text becomes its defining path**, behind the new `canonical_paths: true`
  plan option (off by default, so existing plans are unchanged). A `crate::config::Settings` where
  `crate::config` re-exports `kernel::config` reads `kernel::config::Settings`, so the module can later
  leave its crate. Every path met is named in the run's notes, and a path whose defining module is
  private is left as written and noted.
- **An intra-doc link to the moved item follows it** — a ``[`crate::pairing::f`]`` in a `///` or `//!`
  line reads `crate::answers::f`, for `move_item` and `reparent_module`. Prose, fenced examples and
  `self::`/`super::`/bare links are left alone.

Package entry:
[`tddy-code-restructuring`](../../../../packages/tddy-code-restructuring/docs/changesets/2026-10-06-move-fidelity.md).
