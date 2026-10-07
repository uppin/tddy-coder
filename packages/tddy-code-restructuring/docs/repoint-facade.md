# `repoint_facade_imports`: naming a file's paths by the crate that defines them

How the operation that re-points a file's facade imports is put together. Behaviour is in the
[feature doc](../../../docs/ft/coder/rust-code-restructuring.md#repoint_facade_imports); this page is
where the code is, how a run flows, and what its limits are.

## Shape

A module that is to move into another crate must name what it uses by the crate that **defines** it,
or the move presents an edge back to the crate it leaves. `repoint_facade_imports` applies the
[path survey](path-survey.md)'s answer to one file — or to every file of one module — and writes
nothing else.

The operation is **text-only**: it starts no server and asks none anything. The survey
(`crate_move::survey::survey_moved_file`, made reachable from `backends/rust` by the `pub(crate) mod`
on `crate_move::{survey, reexports}`) is the same textual resolver the cross-crate moves trust, so a
path through a facade is resolved to its defining path without a language server. The dispatch in
`backends/rust.rs` is one `SUPPORTED` entry, one `check` arm that calls `findings`, and one `resolve`
arm placed **before** `self.start(…)` — the operation never reaches the server-start path.

Because resolution is textual, a library-level test drives `RustBackend::resolve` and
`RustBackend::check` over a `fake_lsp`-backed client in milliseconds; one thin live binary runs
`apply` and `check --deep` through the runner with `cargo check --all-targets` as the oracle.

## `backends/rust/repoint_facade/`

| Module | Decides |
|---|---|
| `repoint_facade.rs` | the run: `findings` (the static refusals, from the plan and the text of a `symbol` anchor's file), `resolve_files` (the walk over the files), the `use`-statement and body-path edits, and `notes_of` (the account `check --deep` and `apply` print) |
| `scope.rs` | which files an anchor names — one file for a `symbol` anchor, every file of the module for an `items`/`item` anchor lowered to the `mod` declaration's range (`module_files::files_of`) |
| `rewrite.rs` | `path_edits`: which surveyed paths go through a facade of another crate (`goes_through_a_facade`), and the four preconditions a rewrite is held to; `Rewrite { written, defined_at, line, split_from_group }` |
| `group.rs` | `split_or_reprefix`: one `use` statement's replacement — Rule P in place, Rule S by splitting |
| `refusals.rs` | the refusals that need the code, each naming the path, the file and the line |

## Which paths are rewritten

For each file, with `origin = Destination::read(root, <owning package dir>)` and the file's module path
from `module_path_of`, the file's text is surveyed once. A `SurveyedPath` is rewritten iff

- `defined_at != resolved` — the path the walk reached is not the path as resolved against the file's
  own crate, **and**
- `defining_crate != origin.extern_name` — the crate that defines the item is not the file's own.

So a path through a facade that forwards to a foreign crate is rewritten (`crate::config::X`, a
`self::`/`super::` path, a chain of facades, a facade through a path dependency, an explicit `use` of a
registry crate). A path to an item the crate defines itself, an **in-crate** facade
(`pub use inner::Thing`) and a path already written with a dependency's name are untouched.

The defining path is `defined_at`, the walk's textual answer — child module, defined item, explicit
`use`, then globs that confirm the name, cycle-safe, across path dependencies. A path the walk cannot
see further into comes back as written, so it is not rewritten. This is deliberately not
rust-analyzer's `goto_definition`: a macro-generated or registry-internal re-export is not seen and
stays as written, which is the survey's stated limit.

## The four preconditions

A path that passes the condition above is held to four preconditions, each refused — naming the path,
the file and the line, with nothing written — when it fails:

1. **The defining crate is declared by the package's manifest.** `[dependencies]`, or either table when
   the path sits under `#[cfg(test)]` (`declares_dependency_in_either_table`). A crate the walk
   reached across a path dependency but the package does not name directly is a refusal, not a new
   manifest line — that is the cross-crate moves' manifest pass.
2. **The path is spelled on one line without a comment inside it.** A byte replacement cannot address
   a path split across whitespace or carrying a comment (`spelled_on_one_line`).
3. **In a body, the last segment is unchanged by following the facade.** A facade that renames
   (`pub use a::B as C`) is refused in a body: the token change would be neither a re-point a reader
   expects nor one `verify` excuses.
4. **The rewrite would not bind a name the same scope already binds.** Re-pointing a `use` onto a path
   whose name is already bound in the same scope is `E0252`, so it is refused naming both paths.

## The edits

A path in a **body** (a path in code rather than in a `use` item) is replaced by `defined_at`, and the
rest of the line is untouched.

A plain `use`: the written span becomes `defined_at`, and the visibility, an `as` alias and the `;` are
kept. When the rewrite would change the last segment and the import carried no alias, the old name is
kept with `as <old>` (`plain_path`).

A `use` **group** is rewritten by `group::split_or_reprefix`:

- **Rule P (prefix).** If every leaf agrees on what the group's common prefix becomes, the prefix is
  replaced in place: `use crate::config::{self, X};` → `use kernel::config::{self, X};`, and
  `use crate::{config::A, config::B};` → `use kernel::{config::A, config::B};`. This is the case
  `repointed_header` handles for a move, produced the same way.
- **Rule S (split).** Otherwise — the members disagree — each **lifted** member (one with a rewritten
  leaf) leaves the group and becomes its own statement `<visibility> use <new path>;`; the **kept**
  members stay in the original statement under the original prefix, **first**, and the lifted
  statements follow in member order, one per line, each with the original indentation.
  `use crate::{config::Limits, b::Thing};` → `use crate::{b::Thing};` plus `use kernel::config::Limits;`
  (rustfmt, which `apply` runs over every file it wrote, writes `use crate::b::Thing;`). A group with
  no kept member disappears into its lifted statements.
- A **nested** group member (`a::{x, y}`) is lifted whole when all its leaves share one new prefix, and
  refused otherwise (`nested_member_reaches_two_crates`).

## Refusal classes

At plan read (`plan/codec/facade_imports_fields.rs`, library level), a line that carries any field other
than its anchor — `to`, `name`, `reexport`, `variant`, `type`, `expr`, `order`, `also`, `to_file`,
`with_private_deps`, `callee`, `canonical_paths` — is refused **by name** as one the operation cannot
honour, before the line is deserialized. (`to_type` is refused for every non-`retarget_impl` operation
by the generic retarget rule; `canonical_paths` is refused both here and by the generic rule.)

At `check` / `check --deep` / `apply` (text level, no server), every one a `plan is malformed:`: an
undeclared defining crate; a path spelled across whitespace or a comment; a rename in a body; a
rewrite that would bind a name twice; a `use` group that must split but carries an attribute or doc
comment directly above it; a nested group member whose leaves reach two crates; a range anchor that
covers no `mod` declaration; and a `use` this operation cannot read as one statement.

A plain `check` can examine a `symbol` anchor's file; a module anchor needs `--deep` (it must be
lowered), and the runner's own "anchors by item … run `check --deep`" finding covers it.

## `check --deep` output

Every rewrite is a note, printed by `check --deep` and by `apply` through `console::note`:

```
   note: repoint_facade_imports: 4 path(s) in 2 file(s) go through a facade of another crate
   note:   packages/app/src/a.rs:3: crate::config::Settings -> kernel::config::Settings
   note:   packages/app/src/a.rs:5: crate::config::Limits -> kernel::config::Limits (split out of a grouped `use`)
```

one line per surveyed path rewritten, in file and source order. Refusals are **findings** (non-zero);
the list is not — a survey is "expensive, not defective". Plain `check` prints findings only. The
forwarding is `Rehearsed.notes`, filled from `Resolution.notes`: `Rehearsal` carries every operation's
notes into the deep check, so a `move_item` or `reparent_module` deep check prints its own notes too.

## Idempotence

The rewritten path begins with a foreign crate name, so the walk returns it as written and
`defined_at == resolved`: a second run produces an empty edit and the note
`nothing in <file> goes through a facade of another crate` — a success, not an error.

## Limits

- **No facade is written, removed or edited.** The `pub use` that makes a path forward stays; a later
  `move_*` or a hand edit decides its fate.
- **No manifest edit.** A defining crate the package does not already depend on is a refusal, not a new
  dependency line.
- **No rename of anything.** A facade that renames is kept `as <old>` in a plain `use` and refused in a
  body.
- **Only the file's own crate's facades.** A path through *another* crate's `pub use`
  (`dep::facade::X`) is not followed — `followed` leaves a non-origin path alone.
- **Macro-generated items and paths inside a macro's arguments are not seen**, `#[path = "…"]` modules
  are not followed, and `pub(in …)` is not interpreted — all the survey's limits.
