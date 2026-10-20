# Same-crate moves: `move_item` and `reparent_module`

How the two operations that move code **inside one crate** are put together. Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#same-crate-moves); this page is where
the code is, how a run flows, and what its limits are.

## Shape

rust-analyzer has no assist for either operation, so both are authored here and **engine-informed**, as
the cross-crate moves are: the server answers two questions, and everything else is a function of the
files' text.

| Question to the server | Used for |
|---|---|
| the document outline of the source file | where each moved item starts and ends, its kind (an `impl` and a `mod` are refused), and what the source keeps |
| `textDocument/references` on each moved name (for a module: on the name in its `mod` declaration) | every place that names it, which is what gets re-pointed or measured for a facade |

The moved bytes are copied by range, never printed again, so comments, attributes and formatting
arrive as they were; only the tokens a move must change inside them (a visibility, a relative path, a
qualifier) are edited. Both operations are dispatched from `backends/rust.rs` and nothing else lives there:
two `SUPPORTED` entries, two `check` arms that call each module's `findings`, and two `resolve` arms.
The assembly is a function of texts, so it is unit-tested without a server.

## `backends/rust/item_move/` (`move_item`)

| Module | Decides |
|---|---|
| `item_move.rs` | the run: preflight, outline, survey, assemble, then one `FileEdit` per file (a `Create` first for a created module) turned into minimal edits |
| `preflight.rs` | what the plan and the text already say, before any server: the destination (`named_by`, the package and module path), a missing destination, a name already declared (leaving out the destination's own import of the moving item, also through a glob re-export), the items' own module |
| `creation.rs` | the module a line with `name` creates: the parent must exist and not declare the name, the file is `<parent dir>/<name>.rs`, the declaration is inserted below the parent's last `mod` |
| `destination.rs` | finding a module from its path by following `mod` declarations from `src/lib.rs` (then `src/main.rs`); the crate root is the empty path |
| `outline.rs` | the run of items an anchor covers, from the outline; refuses a range inside an `impl`, a cut item, a module among the items, and a keyword on a line above its name |
| `sites.rs` | the places that name a moved item, read around each reference position (a `use`, a qualified path, a bare name through an import); refuses a name in a nested `use` group. A caller that wrote a one-segment module qualifier bound by a `use` in scope keeps that shape — the destination's last segment is bound with a new `use` and the qualifier rewritten to it — unless that name is already taken, when today's full path is written and noted |
| `scope.rs`, `reach.rs` | visibility read as a module subtree, so a keyword is respelled for the module it lands in and widened only as far as callers, imports and the moved code's own reach need; a created module's declaration and the declarations on the path to the destination are widened the same way |
| `rebase.rs`, `bindings.rs` | relative spellings in moved code (`super::f()`, `pub(super)`) keep meaning what they meant. A `super::Name` that reaches a module which only imports `Name` is kept when the import is visible at the destination or the destination is inside the module, and otherwise followed one hop to what the import brings in: a plain, aliased or confirmed-glob import of this crate, or another crate's path (rooted with `::` when the destination binds the crate's name). A glob that cannot confirm the name is refused, naming the file and line |
| `use_path.rs` | the one Rust 2018 reading of a `use` path: `crate`/`self`/`super` against the module that writes it, a head the module binds as local, any other head an extern crate. Read by the clash check, the one-hop lookup and `retarget_impl`'s S6 |
| `imports.rs` | the source module's `use` header copied across (head from the crate root, bound names left out, unreachable modules dropped or widened, a group split per unbound name) |
| `placement.rs`, `text.rs` | where the text lands in the destination (a file or an inline module; one written on one line is refused) and the byte-offset editing and masked-text readings shared by both operations |
| `facade.rs` | the `pub use` a move leaves: `glob` is one line over the destination, `named` and `outside` one grouped line per visibility |
| `outside.rs` | the one decision `reexport: outside` takes for both operations: which sites lie outside the crate (see below) |
| `assemble.rs` | the new text of every file, from the original texts and the server's answers |
| `members.rs` | the fields, methods and associated constants a move splits from their users: `members_of` reads them from the outline (the children of a struct and of an **inherent** `impl`; never a trait `impl`, a trait, an enum, or a member written `pub`/`pub(crate)`/relative), `widen_members` computes each one's scope from the modules of its references, `member_visibility_edit` writes the keyword before the member's name (the one edit `outline::visibility_edit` refuses for a field), `landing_of` adds both to the item visibilities `assemble` computes. Two sets: moved members (widened where they land, for users outside the run) and kept members (widened where they stay, for users inside the run, only when the moved text mentions the name). Users are keyed by owner type and member name |
| `canonical_paths.rs` | `canonical_paths: true` on a `move_item` line: each `crate::`-headed path in the moved text whose defining path differs is rewritten to it, every path rewritten or left is noted, and a path whose defining module is private is left as written |
| `doc_links.rs` | the `///` / `//!` link pass: a fully-qualified link that names or reaches a moved item/module is respelled; prose, fenced examples and `self::`/`super::`/bare links are left alone |

## `backends/rust/module_reparent/` (`reparent_module`)

| Module | Decides |
|---|---|
| `module_reparent.rs` | the run, with the same shape as `item_move.rs`; the callers come from `callers_of_the_module` |
| `reading.rs` | what the plan says: the new parent (`named_by`, shared with `move_item`) and the one module the anchor's `mod` declaration names |
| `survey.rs` | the lexical findings, before a server: new parent missing or inline, name already declared there, destination inside the moved module, an inline module, `#[path]`, a declaration with no file, a target file that exists |
| `declaration.rs` | reading `mod name;` as whole lines (attributes and doc comments above, visibility, `;`), and where a declaration belongs in the new parent |
| `relocation.rs` | where each file goes: it keeps its place relative to the module, under the new parent's directory |
| `visibility.rs` | the declaration's visibility respelled for the new parent, as `move_item` respells an item's |
| `tree_reach.rs` | what the moved tree names in the places it leaves: `modules_to_survey` gives the old parent and each ancestor below the common ancestor of the old and new parents, `reached_by_the_tree` reads each module's own file for root items written private or restricted whose scope does not cover the new module path, keeps the ones the moved files mention, confirms each with `references_at`, and `widenings` widens them with `Scope::widened_to` |
| `tree_visibility.rs` | every `pub(…)` written in a moved file, read at its old module: `pub(crate)` and a scope inside the tree are left alone, a scope that reached outside is widened until legal under the new parent, an unreadable one is refused naming file and line. No server request |
| `assemble.rs` | the new text of every file and the `Rename` for every moved file |

`crate_move/module_files.rs` is the shared reading of the files a module spans: `children_directory` (a
crate root and a `mod.rs` own their directory, any other `a.rs` owns `a/`), `file_of_child` (`<name>.rs` or
`<name>/mod.rs`) and `files_of` (the module's file and every file its `mod` declarations lead to,
inline modules followed, refused when a declaration leads to no file), and `relocated`, the one rule for
where each of those files lands under a new directory (`MovedFile { from, to, below }`). `reparent_module`
(`module_reparent/relocation.rs`) and the cross-crate moves (`Move::carried`) both call it. It is read
from the workspace, so an earlier operation's pending edits are seen.

## Emptied directories

`apply::remove_emptied_directories` runs at the end of `apply_workspace_edit` over the `from` of every
`Rename`: for each, it removes the parent chain while a directory exists, lies strictly below the root and
`read_dir` yields nothing, and stops at the first one that is not empty. `PreImage::restore`
(`journal/group.rs`) calls it after removing a file a group created, and already recreates the parent of a
file it restores, so a rolled-back or re-run operation finds the directories it writes into. An I/O error
other than a missing directory is returned. The dry-run overlay models files, not directories.

## Plan codec

`plan/refactor_kind.rs` has `RefactorKind::{MoveItem, ReparentModule}`; `plan.rs` has `Reexport::Outside`; neither kind satisfies
`moves_across_crates`. `plan/codec.rs` reads: `to` is required; the anchor must be `items` or `item`;
`reexport` is allowed on both (and `outside` only on both); `name` on `reparent_module` and `named` on it
are refused; `also` and `to_file` are refused on both. `Reexport::repoints_callers` is true for `none` and
`outside`. `RefactorOp.name` on a `move_item` names the module the move creates. `canonical_paths` is a
`move_item`-only boolean, off by default (`plan/codec/canonical_paths.rs` refuses it on every other
operation).

## Output fidelity: caller imports, facade paths, doc links

Three ways a `move_item` result is made to read as a hand edit, each with a test in
`move_fidelity_acceptance`:

- **A caller keeps its short qualifier.** A caller that reached the item through a one-segment module
  qualifier bound by a `use` in its scope (`use crate::pairing; … pairing::f(..)`) gets the
  destination's last segment bound with a new `use` and the qualifier rewritten to it
  (`use crate::answers; … answers::f(..)`), not the full `crate::answers::f(..)` inlined. The old `use`
  is left for the end-of-run tidy to prune when nothing else needs it. If the destination's last
  segment is already taken in the caller's scope, today's full path is written and noted.
- **A facade path in the moved text becomes the defining path** — behind `canonical_paths: true` (off
  by default, so existing plans are unchanged). A `crate::config::Settings` where `crate::config` is a
  `pub use kernel::config;` reads `kernel::config::Settings` in the destination. Every path met,
  rewritten or left, is named in the run's notes; a path whose defining module is private is left as
  written and noted. This reuses `crate_move`'s path survey, which is why `crate_move::survey` and
  `crate_move::reexports` are `pub(crate)`.
- **An intra-doc link to the moved item follows it.** A ``[`crate::pairing::f`]`` in a `///` or `//!`
  line reads `crate::answers::f` after the move, for `move_item` and `reparent_module`. rust-analyzer
  returns no reference for a doc link, so this is a text pass (`item_move/doc_links.rs`); prose,
  fenced examples and `self::`/`super::`/bare links are left alone.

## `reexport: outside`

`outside::Reach::of` splits the sites the server found by the package that owns each file, read from the
manifests (`package_of`), and by the target: only the library crate (files under `src/`) is inside. In
the same package, `tests/`, `examples/`, `benches/`, and `src/main.rs` or `src/bin/**` when a `src/lib.rs`
exists, are outside. The inside sites are re-pointed as `none` re-points them; the names the outside sites
reach are the only ones a facade is written for (`facade_items`); the outside sites are never edited.
`facade_lines` of `extract_module` refuses `Outside`, and `crate_move` treats it as unreachable, because
the codec refuses it for every other operation.

## Ergonomics that came with it

- `item_anchor::owning_package` (now `pub(crate)`) adds `repo_root_hint` to its "is in no package" refusal:
  when `<package dir>/<file>` exists below one package, or several, the refusal names the repo-root
  path(s) to write. The path is not resolved for the author.
- `item_anchor::plan_file_has_item_anchors` is the one reading of "does this plan file hold item anchors",
  used by `restructure_cli::names_item_anchors`, by `tddy-tools`' routing of `snapshot` and by the
  index daemon's `Snapshot`.
- `console::snapshot_lines` renders a snapshot from its parts, so a snapshot served by the daemon reads as
  one run in process.
- `RestructureCommand::Warm` and `Command::Warm` exist, and `dispatch` answers it in process with
  `RestructureError::WarmNeedsIndexDaemon`: a crate graph loaded by a process that exits is dropped with
  it, so only a daemon can hold one.

## Tests

`tests/same_crate/mod.rs` holds the fixture builders both families use (`a_move_item_op`,
`a_reparent_module_op`, `the_anchor_over`), so `tests/harness/mod.rs` is not grown. The suites, each over
a generated workspace and a live rust-analyzer, with `assert_compiles_with_its_tests` and
`assert_lints_clean` as oracles: `move_item_acceptance`, `move_item_beyond_the_basics_acceptance`,
`move_item_creates_module_acceptance`, `move_item_into_an_existing_module_acceptance`,
`move_item_of_an_item_the_destination_imports_acceptance`, `move_item_outside_facade_acceptance`,
`move_item_paths_acceptance`,
`reparent_module_acceptance`, `reparent_module_beyond_the_basics_acceptance`,
`reparent_module_through_the_old_parents_import_acceptance`, `same_crate_deep_check_acceptance`,
`same_crate_widening_acceptance`, `move_fidelity_acceptance` and
`anchors_package_relative_path`. `same_crate_widening_acceptance` covers the member and tree widenings and
the emptied directories; the member widening, the tree visibilities, the survey bound and the sweep are
also unit-tested in their modules, from outline JSON and texts, with no server. Every live
rust-analyzer suite of the package is listed in `.config/rust-e2e.filterset` and the `rust-analyzer`
group of `.config/nextest.toml`, which runs them one server at a time.

## Limits

Each is a refusal or a compile-gate failure; none is silent. They are listed in the feature doc's
[Known limitations](../../../docs/ft/coder/rust-code-restructuring.md#known-limitations): members are
surveyed in the source file only (an `impl` of the moved type in another file, and a tuple-struct field the
outline does not list, are not), a re-parented tree's reach is surveyed in the root items of the old parent
and its ancestors (an item inside a sibling module is not), the `use` header is copied whole, and `outside` reads the default target layout only.
