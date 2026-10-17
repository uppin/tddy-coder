# 2026-10-17 — A crate move creates its destination crate

`move_module_to_crate` and `move_cluster_to_crate` create the destination crate when the plan line carries `name`:
the manifest (the origin's `version` and `edition`, verbatim), `src/lib.rs` and the workspace `members` entry are
part of the move's single edit, and a later operation of the plan moves into the crate an earlier one created.
`check`, `check --deep`, `apply --dry-run` and `apply` agree on such a plan, and a plan that cannot create the crate
is refused naming the path, with nothing written. No hand-written crate skeleton is needed.

The destination's manifest also gains the crates the moved code names in two shapes it used to miss: a crate
imported under its own name (`async-trait`, `anyhow`), and a crate the origin declares only under a target table
(`libc` under `[target.'cfg(unix)'.dependencies]`), which lands under the same table.

See [Rust code restructuring](../rust-code-restructuring.md#crate-creation).
