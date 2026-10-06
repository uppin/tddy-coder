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
| `sites.rs` | the places that name a moved item, read around each reference position (a `use`, a qualified path, a bare name through an import); refuses a name in a nested `use` group |
| `scope.rs`, `reach.rs` | visibility read as a module subtree, so a keyword is respelled for the module it lands in and widened only as far as callers, imports and the moved code's own reach need; a created module's declaration and the declarations on the path to the destination are widened the same way |
| `rebase.rs`, `bindings.rs` | relative spellings in moved code (`super::f()`, `pub(super)`) keep meaning what they meant; a `super::Name` that reached a plain private `use` is respelled to the item it stands for |
| `imports.rs` | the source module's `use` header copied across (head from the crate root, bound names left out, unreachable modules dropped or widened, a group split per unbound name) |
| `placement.rs`, `text.rs` | where the text lands in the destination (a file or an inline module; one written on one line is refused) and the byte-offset editing and masked-text readings shared by both operations |
| `facade.rs` | the `pub use` a move leaves: `glob` is one line over the destination, `named` and `outside` one grouped line per visibility |
| `outside.rs` | the one decision `reexport: outside` takes for both operations: which sites lie outside the crate (see below) |
| `assemble.rs` | the new text of every file, from the original texts and the server's answers |

## `backends/rust/module_reparent/` (`reparent_module`)

| Module | Decides |
|---|---|
| `module_reparent.rs` | the run, with the same shape as `item_move.rs`; the callers come from `callers_of_the_module` |
| `reading.rs` | what the plan says: the new parent (`named_by`, shared with `move_item`) and the one module the anchor's `mod` declaration names |
| `survey.rs` | the lexical findings, before a server: new parent missing or inline, name already declared there, destination inside the moved module, an inline module, `#[path]`, a declaration with no file, a target file that exists |
| `declaration.rs` | reading `mod name;` as whole lines (attributes and doc comments above, visibility, `;`), and where a declaration belongs in the new parent |
| `relocation.rs` | where each file goes: it keeps its place relative to the module, under the new parent's directory |
| `visibility.rs` | the declaration's visibility respelled for the new parent, as `move_item` respells an item's |
| `assemble.rs` | the new text of every file and the `Rename` for every moved file |

`crate_move/module_files.rs` is the shared reading of the files a module spans: `children_directory` (a
crate root and a `mod.rs` own their directory, any other `a.rs` owns `a/`), `file_of_child` (`<name>.rs` or
`<name>/mod.rs`) and `files_of` (the module's file and every file its `mod` declarations lead to,
inline modules followed, refused when a declaration leads to no file). It is read from the workspace, so
an earlier operation's pending edits are seen.

## Plan codec

`plan/refactor_kind.rs` has `RefactorKind::{MoveItem, ReparentModule}`; `plan.rs` has `Reexport::Outside`; neither kind satisfies
`moves_across_crates`. `plan/codec.rs` reads: `to` is required; the anchor must be `items` or `item`;
`reexport` is allowed on both (and `outside` only on both); `name` on `reparent_module` and `named` on it
are refused; `also` and `to_file` are refused on both. `Reexport::repoints_callers` is true for `none` and
`outside`. `RefactorOp.name` on a `move_item` names the module the move creates.

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
`reparent_module_acceptance`, `reparent_module_beyond_the_basics_acceptance`,
`reparent_module_through_the_old_parents_import_acceptance`, `same_crate_deep_check_acceptance` and
`anchors_package_relative_path`.

## Limits

Each is a refusal or a compile-gate failure; none is silent. They are listed in the feature doc's
[Known limitations](../../../docs/ft/coder/rust-code-restructuring.md#known-limitations): fields and `impl`
members split by a move and the private items of a re-parented tree are not widened, the `use` header is
copied whole, an aliased import of the moving item reads as a name clash, and `outside` reads the
default target layout only.
