# Crate creation

A crate move creates its destination when its plan line carries `name`. The product contract is
[Rust code restructuring](../../../docs/ft/coder/rust-code-restructuring.md#crate-creation).

```jsonl
{"op":"move_cluster_to_crate","anchor":{ … },"also":[{ … }],"to":"packages/tddy-restructure-plan","name":"tddy-restructure-plan","reexport":"glob"}
{"op":"move_module_to_crate","anchor":{ … },"to":"packages/tddy-restructure-plan","reexport":"glob"}
```

The first line creates `packages/tddy-restructure-plan`. The second moves into the crate the first
created and carries no `name`.

## Where it lives

| Piece | Where |
|---|---|
| `NewCrate { dir, package }`, `refusal`, `skeleton`, `folded`, `created_by_earlier_operations` | `crate_move/new_crate.rs`; `NewCrate` is re-exported from `lib.rs` |
| the codec's refusals over the whole plan | `plan/codec/crate_move_fields.rs`, called from `parse_ops` |
| `Destination::read_in` | `crate_move/destination.rs` |
| one creation per cluster | `MovingCluster.creates`, set by `cluster::named_by` |

## The plan line

`name` is the new crate's `[package] name` on `move_module_to_crate` and `move_cluster_to_crate`. The codec
refuses it, reading the whole plan, when:

- it is on `move_test_binary_to_crate`, which moves into a crate that exists
- it is not a Cargo package name: a non-empty run of ASCII letters, digits, `-` and `_` that does not start
  with a digit or `-`, and is not `test`
- two operations of one plan carry `name` with the same `to`

Elsewhere `name` keeps its meaning (`move_item`) or its refusal (`reparent_module`, the signature
operations).

## Refusals that need the tree

`NewCrate::refusal` is static, so plain `check` reports each one. Nothing is written, and the message
names the path:

- `<to>/Cargo.toml` exists: `<to>` is already a crate, so drop `name` to move into it
- `<to>` exists and holds any entry
- the extern name (`-` read as `_`) equals the origin's, or that of a crate the root `members` array lists
  explicitly. Glob entries are not expanded, so a clash through one surfaces at the compile gate, where
  cargo refuses two packages of one name.
- the root `Cargo.toml` has no `members` array, so cargo would not build the crate

A move **without** `name` into a missing crate is refused as "not a crate" and says that `name` creates it.

## The skeleton

Two `FileEdit::Create` entries, `<to>/Cargo.toml` and `<to>/src/lib.rs`, and this manifest:

```toml
[package]
name = "<name>"
<origin's version line, verbatim>
<origin's edition line, verbatim>
```

The lines are copied as written, so `version.workspace = true` stays that way. An origin without one omits
that line; it is never invented. `src/lib.rs` starts empty, and nothing else is authored: no
`description`, no `[lib]`, no `publish`, no version or feature list for a dependency.

## What fills it

The move resolves exactly as it does into an existing crate, against a workspace whose overlay already
holds the skeleton: `declared_in_destination` inserts each `pub mod <m>;` sorted, `destination_manifest`
appends `[dependencies]`, `[dev-dependencies]` and target tables, and `workspace_members` appends the
destination to the root `members` array (appended, not sorted). A cluster creates its crate once, before any
member resolves, and its members are declared in module-path order.

A created file's edits are folded into its skeleton text (`new_crate::folded`), and the file is emitted as
one `Create` plus one `Change` inserting the final text at 1:1. No path gets two `Change`s, so the
position ledger, the overlay and the journal see one edit per file.

## Plan order

`Destination::read_in(workspace, dir)` reads the destination's manifest through the workspace overlay, so a
later operation finds the crate an earlier one created. `check --deep` and `apply --dry-run` already fold
each operation into their overlay. Plain `check` builds, for operation N, an overlay holding the skeletons of
every earlier operation's creation (`created_by_earlier_operations`; a creation that is itself refused is
left out) and checks N against it. `check`, `check --deep`, `--dry-run` and `apply` therefore agree on a plan
that creates a crate and then moves more into it, and the stranded-sibling check reads each operation
against the crates earlier operations create. `move_test_binary_to_crate` reads the destination from disk
and creates nothing.

## Testing

`tests/new_crate_acceptance.rs` drives the plan codec and the static check through the public API.
`crate_move/new_crate_tests.rs` and `crate_move/manifest_pass_tests.rs` run in the crate over a fake
reference set, asserting the created text, the folding, the plan-order read and the manifest lines copied
for `async-trait`, `anyhow` and `libc`. One live test, in `tests/move_module_to_crate_acceptance.rs`, moves
a module into a crate the move creates and ends in `cargo check`. Registry crates are never compiled: the
live fixture keeps to path crates.
