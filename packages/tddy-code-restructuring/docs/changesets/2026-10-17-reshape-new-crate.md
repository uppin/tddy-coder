# 2026-10-17 — A crate move creates its destination and carries every crate the moved code names

**Type:** Feature

`#reshape` 8/19 (PR #606). `move_module_to_crate` and `move_cluster_to_crate` accept `name`, the
`[package] name` of a crate the move creates at `to`; the manifest pass stops missing two kinds of crate.

- **Creation.** New `crate_move/new_crate.rs` (`NewCrate`, `refusal`, `skeleton`, `folded`,
  `created_by_earlier_operations`) and `plan/codec/crate_move_fields.rs`. The skeleton is `[package]` with the
  origin's `version` and `edition` lines verbatim, an empty `src/lib.rs` and a root `members` entry, folded into
  one `Create` and one `Change` per file. A cluster creates its crate once (`MovingCluster.creates`). Refusals:
  the codec's (`name` on `move_test_binary_to_crate`, a non-package name, a `to` created twice) and `check`'s
  (`to` already a crate or not empty, an extern-name clash with the origin or a listed member, no `members`
  array). A move without `name` into a missing crate says `name` creates it.
- **Plan order.** `Destination::read_in` reads the destination through the overlay; plain `check` seeds the
  crates earlier operations create, so `check`, `check --deep`, `--dry-run` and `apply` agree on a plan that
  creates a crate and fills it.
- **Self-named imports** (`survey.rs`, `names_bound_by`): `use async_trait::async_trait;` is surveyed as the
  crate when the origin declares it and nothing else binds the name.
- **Target tables** (`manifest_edits/target_tables.rs`, `Move::dependency_lines` returning `CarriedLines`): a
  crate the origin declares only under `[target.'cfg(unix)'.dependencies]` is surveyed in bodies, no longer
  refused in `use`, and lands under the same header in the destination; test-only code lands in the target
  `dev-dependencies` table.
- **File length.** Engine-driven `move_item` splits kept the files under budget: `manifest_edits.rs` 549 to 479
  production lines (+ `manifest_edits/target_tables.rs`, 77), `moving.rs` 517 to 424 (+ `moving/callers.rs`, 85;
  `moving/table_edits.rs`, 20).

Backlog entries resolved: `2026-10-08-restructure-move-to-crate-misses-a-crate-named-only-by-an-attribute-macro`
(the cause was `use async_trait::async_trait;` binding its own head, not the attribute; deleted). Also resolved
by this change: the create-crate, `async-trait` and `libc` items of
`2026-10-08-restructure-move-cluster-ignores-also-members-that-are-directory-children-and-their-crates`, and
`2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind`
(the `libc` target-table defect).

Not changed: `move_test_binary_to_crate` (no creation, no target tables), `backends/rust.rs`, `test_binary.rs`.
Code issues: none of the open records in `docs/code-issues/` covers a file this change touched, so none was
re-measured or changed. Technical debt: `moving.rs::dependency_lines` grew from 49 to 57 lines and the new
`target_lines` is 43, past the 40-line guideline. Related nodes: #605 (7/19, grouped `use`).

Scoped gate, `tddy-code-restructuring`: 82 test binaries, 1629 passed, 0 failed; clippy and fmt clean.
Documentation: [crate-creation.md](../crate-creation.md), [path-survey.md](../path-survey.md),
[Rust code restructuring](../../../../docs/ft/coder/rust-code-restructuring.md#crate-creation).
