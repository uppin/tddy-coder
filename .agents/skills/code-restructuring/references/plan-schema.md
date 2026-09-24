# Plan schema

A plan is JSONL. Line 1 is the snapshot header; every later line is one operation, applied in order.

```jsonl
{"v":1,"snapshot":{"<path>":"sha256:<digest>", …}}
{"op":"<name>","anchor":{…}, …}
```

The plan is a **command log** and is never rewritten. Execution appends to `.restructure/journal.jsonl`
(the event log) and checkpoints the coordinate ledger to `.restructure/ledger.json`.

## Anchors

Both kinds are expressed in the coordinates of the snapshot, never adjusted for earlier operations.

```jsonc
{"kind":"symbol","file":"src/shapes.ts","path":"scaleBox"}   // preferred; survives edits above it
{"kind":"range","file":"src/shapes.ts",
 "start":{"line":412,"col":5},"end":{"line":468,"col":6}}     // one-based line and column
```

## Operations

| `op` | Anchor | Fields | TypeScript | Rust |
|---|---|---|---|---|
| `extract_method` | range | `name`, `variant` = `inner` \| `module` | ✅ | ✅ |
| `extract_variable` | range | `name` | ✅ | ✅ |
| `extract_type` | range | `name`, `variant` = `alias` \| `interface` | ✅ | — |
| `extract_class` | range over whole members | `name`, `to` | ⚠️ | — |
| `move_symbol` | symbol | `to`, `with_private_deps` | ✅ | — |
| `move_file` | symbol | `to` | ✅ | — |
| `rename_symbol` | symbol or range | `name` | ✅ | ✅ |
| `extract_module` | range over a selection of items | `name`, `reexport` = `glob` \| `named` \| `none`, `to_file` | — | ✅ |
| `extract_module_to_file` | range at the `mod` keyword | — | — | ✅ |
| `move_module_to_crate` | symbol | `to`, `reexport` = `glob` \| `none` | — | ✅ |
| `move_cluster_to_crate` | symbol (first member) | `also` (the other members' anchors), `to`, `reexport` | — | ✅ |
| `move_test_binary_to_crate` | the test binary's path | `to` | — | ✅ |
| `extract_trait` | range at the `impl` keyword | `name` | — | ✅ |
| `inline_method` | symbol | — | — | ✅ |
| `organize_imports` | symbol | — | ✅ | — |
| `add_missing_imports` | symbol | — | ✅ | — |

Every operation but one is backed by a real assist in the engine that claims it. Where a cell is
empty the engine has no equivalent, and the executor refuses the operation rather than approximating
it. The ⚠️ marks the exception — see **`extract_class` is hand-written** below.

`variant` selects between actions the engine offers for the same operation. Omitting it keeps the
behaviour an operation had before the field existed (`inner`, `alias`), and a name the operation does
not define is refused rather than ignored.

### Notes that matter when planning

- **`move_symbol` carries private-only dependencies with the symbol.** TypeScript always does this;
  `with_private_deps: false` is refused rather than silently ignored.
- **Rust has no whole-symbol move.** To split a Rust file, group the items with `extract_module` and
  then run `extract_module_to_file`, whose anchor is a caret on the `mod` keyword.
- **A mutually-referencing set moves with `move_cluster_to_crate`, not with several
  `move_module_to_crate` ops.** `anchor` is the first member and `also` carries the rest, as anchors
  of the same shape. The whole set resolves into **one** edit, so the tree is never half-moved — and
  that is not a nicety: moved one at a time, each module's reference to a sibling still in the origin
  makes the destination depend on the crate it left, and the operation refuses. No ordering fixes a
  cycle, which is why layering a plan by dependency depth cannot substitute.

  A path reaching a **co-moving** member keeps `crate::` — the destination *is* `crate` for a file
  that has arrived in it, and naming the destination crate there is `E0433` plus a self-dependency.
  A path reaching a module **staying behind** is re-pointed at the origin, exactly as a single move
  does. A set of one is refused: that is `move_module_to_crate`.

  A plain `check` names the sibling a **partial** set would strand, statically and with no index —
  which is the finding that used to be discovered only when `apply` refused.
- **A test binary moves with `move_test_binary_to_crate`, never with `move_module_to_crate`.** The
  anchor is `<crate>/tests/<name>.rs` and `to` is the destination crate's directory. `reexport` is
  refused — nothing can reference a test binary, so a facade would keep nothing resolving — and
  there is no origin edit at all, because cargo auto-discovers `tests/*.rs` and the crate the test
  left never named it. The destination gains `[dev-dependencies]`, not `[dependencies]`.

  Every path in the moved file that opens with the origin's extern name is re-pointed, bodies
  included, and each is resolved to the crate that **defines** what it reaches rather than to the
  first crate that re-exports it. `crate::`, `super::` and the file's own `mod` names are left
  alone: a test binary already is its own crate root, so they mean the same thing in the
  destination. A path continuing into a group or a glob (`origin::{a, b}`) is refused — split the
  declaration first. String literals are never rewritten, so a `CARGO_MANIFEST_DIR`-relative path to
  a sibling crate's source is a hand edit after the move. `tests/common/mod.rs` is not a test binary
  and is refused as an anchor; it is a module of whichever binaries declare it.

- **`to_file: true` does both steps in one operation, and is the way to do this.** `extract_module`
  groups the items and gives the module its file without ever naming the intermediate `mod` keyword, so
  the constraint below does not arise. The parent's edit is measured against what was on disk, so one
  journal entry describes the whole operation and the ledger still sees a single edit. A plan written
  before the field existed produces a byte-identical tree, and an operation that cannot honour it is
  refused rather than having it ignored. The four-plan recipe becomes two plans, and two cold indexes
  instead of four.
- **Without `to_file`, those two steps need two plans, not two lines of one plan** — but *only* those
  two. The `mod` keyword the second step aims at is text no original coordinate maps to, so the ledger
  correctly reports the anchor as inside removed text. Apply the first plan, then start the second as a
  restructure of its own: clear `.restructure/`, re-hash the file, and write the anchors against the
  tree the first plan left. Run state describes one plan's edits, and a journal left in place would
  translate the new plan's coordinates a second time.

  Other Rust operations compose in one plan, **in the right order**. They once did not at all, for a
  reason that was nothing to do with anchors: the backend reported each operation as a single edit
  spanning from its first changed line to its last, so an extraction that rewrote two distant places
  swallowed every untouched line between them. It now reports one hunk per changed region, so the
  lines between stay addressable and a plan can carry as many seams as you can order correctly.

  **Several `extract_method`s in one function compose only bottom-up**: last range first, so every
  operation's anchor sits *above* every edit an earlier one made. Each extraction replaces its
  statements with a call and writes the new function below the one it came from, so taken bottom-up
  no anchor is ever translated through another extraction. Taken top-down, every later anchor has to
  be, and on the lifecycle destructure's plans (2026-09-23) those plans did not apply. The engine
  does not re-anchor them for you.

  **An `extract_method` range may not hold an early exit of the function around it.** rust-analyzer
  copies a `return` in the range verbatim into the new function, whose return type is not the
  caller's: plan 10 of the destructure applied six such extractions over `start_session_core` and left
  seven `E0308`s (and where the types happen to agree, the caller's exit is silently skipped). The
  backend refuses such a range as `this seam cannot be cut here:`, naming each line, before a server is
  asked — so a plain `check` reports it too. A `return` inside a closure, an `async` block or a nested
  `fn` within the range leaves that body, not the caller's, and is allowed. The check is lexical:
  strings and comments are masked first, and a `return` a macro expands to (`bail!`) is not seen —
  `apply`'s compile gate catches what that leaves.
- **`apply` is judged by the compiler.** After its operations, `apply` runs `cargo check
  --all-targets` over every package owning a file it changed (test targets included, because moves
  re-point imports tests use). A failure fails the run with the compiler's errors; the edits stay on
  disk and in the journal for inspection, and the message names the touched paths and the journal to
  remove when rolling back. A fresh apply first checks the packages owning the files the plan names,
  and refuses — writing nothing — when that baseline already fails, so a pre-broken tree is never
  blamed on the plan. A dry run, and a run continuing a journal, skip the baseline.
- **A degraded index is refused.** rust-analyzer finishes loading even when a build script failed,
  and then answers as though the generated code did not exist (`req: _`, imports it cannot find). It
  says so only through its `experimental/serverStatus` health; any health but `ok` — `warning`
  included — refuses the run as `rust-analyzer's answer was unusable:`, quoting the server's message.
  This holds against a warm `tddy-index-daemon` too.
- **`extract_module` restores the imports its own assist loses, and refuses when it cannot.** The
  items move out of the scope of the file's `use` declarations, so the backend asks rust-analyzer for
  an import at each name left unresolved. Where the server offers several paths for one name, the
  file's existing imports decide — by an exact path match, then by a module the file already imports
  from, then by the **crate** the file binds that same name from. Where none of the three settles it,
  the operation refuses and names the candidates rather than guessing.

  The crate tier exists because a re-export gives one item two paths. `tddy-core` publishes
  `pub use error::{BackendError, ParseError, WorkflowError};`, so a file writing the canonical
  `tddy_core::error::ParseError` is offered the shorter `tddy_core::ParseError` — the same type under
  a different string, which neither of the first two tiers can match. Before the crate tier, one
  extraction was refused three candidates deep over exactly that.

  **Two candidates rooted in the same crate is still a refusal**, and so is a name the file does not
  already bind. Qualifying the name fully inside the anchored range settles it either way, but reach
  for that only after reading the refusal: it writes a fully-qualified path into production code
  permanently.
- **`extract_module` refuses when another file references what would move — unless you ask for a
  facade.** The module is reached by a different path than the items are now and rust-analyzer
  rewrites no reference it did not move, so a caller elsewhere would only surface at compile time.
  The refusal names each item and the files referencing it.

  `reexport` is the cheap way past it, and it touches no caller: `"glob"` leaves one
  `pub use <name>::*;` in the parent, so `crate::core::context_manager::create_shading_pdf` goes on
  resolving through the re-export. `"named"` leaves one grouped `use` per visibility tier, covering
  only the items something outside the new module reaches. Absent, or `"none"`, is the refusal above.
  A parent `foo.rs` may own a `foo/` directory in Rust 2018, so the facade survives a later
  `extract_module_to_file` with no rename to `mod.rs`.

  **A `named` facade that names nothing is reported, not refused.** A seam whose items are all reached
  only from inside it legitimately needs no facade, so the operation succeeds and says so on stdout as
  a `note:` line — a facade asked for and absent is otherwise visible only in the diff.

  **A `named` facade refuses an item whose own module it would not carry.** A nested item is reached
  *through* the module holding it, so re-exporting that module keeps `parent::nested::buried`
  resolving and naming the item as well would publish `parent::buried`, a path no caller used. Where
  something outside reaches the nested item but nothing reaches its module, no line would keep the old
  path resolving and the operation refuses — `reexport: glob` covers that case, because a glob
  re-exports the module too.

  The refusal only ever concerned items reached by a **module path** — free functions, structs, enums,
  statics, consts, type aliases. An inherent associated function or method is reached through its
  *type*, so the `impl` carrying it can move to any module in the crate with no caller anywhere
  changing. **Moving a whole `impl` is free of caller churn**, and is the cheapest restructuring move
  Rust has.

  **Free of callers is not free of seams, and the difference decides whether a plan runs.** Four
  geometries look alike and only one blocks — measured, not assumed:

  | Geometry | Outcome |
  |---|---|
  | A whole `impl` moves; the parent calls its methods | **Succeeds.** A method is reached through its type, so there is nothing to rewrite |
  | A path-reached item moves; the parent still names it | **Succeeds.** The assist rewrites the reference and the import pass restores the binding — which is why an in-file reference "costs nothing" |
  | Some members of an **inherent** `impl` move while a sibling left behind calls them | **Succeeds.** The assist writes `mod … { use super::Gauge; impl Gauge { … } }`, so they stay methods of the same type. It also inserts `modname::` before the moved member's name in every call to it, none of which is Rust: `self.modname::doubled()` and `Self::modname::doubled(…)` from a member left behind, `Gauge::modname::doubled(2)` (or through a type alias) from the file's `mod tests`. The backend undoes exactly those rewrites — the placeholder after a `.` or after a type qualifier, never after `super`/`self`/`crate`, which reach a moved *free* item and are the rename's. A private member comes out `pub(crate)` (below) |
  | **Some members of a trait `impl` move while a sibling left behind calls them** | **Refused.** The new module would hold a second `impl Meter for Gauge` (`E0119`) and each half would lack the other's items (`E0046`) |

  Only the last is a blocker, and no ordering fixes it: an `impl` body cannot hold a `mod`, so the
  sibling can be moved neither out of the way first nor after. Grow the seam to carry the whole `impl`,
  or cut it where nothing crosses. The refusal fires before the assist runs and names the member, the
  `impl` it belongs to, and the lines its siblings reach it from.

- **`extract_module` reports every visibility it had to widen, and preserves the rest — except inside
  an `impl`, where it reports but does not restore.** The assist rewrites what it relocates to
  `pub(crate)`. For a **path-reached** item, what nothing outside the new module reaches is put back as
  it was written, so a seam that carries a private helper along with its only caller keeps the helper
  private. Prefer such seams: cutting between a helper and its only caller is what forces a widening.

  A relocated **`impl` member** is different, and deliberately so. It is reached through its type, so
  no module path names it and the survey that decides restoration rightly does not descend into an
  `impl` — which means the member stays `pub(crate)`. That widening is now *named* on stdout and
  recorded on the journal entry, but it is not undone: narrowing it back would be `E0624` for a private
  method with a sibling-module caller, which is safe today precisely because the item stays widened.
  So **expect a private method to come out `pub(crate)` and stay there**, and account for it as a real
  consequence of the seam rather than looking for a way to prevent it.
- **`extract_module` refuses a module name the parent has already taken.** The name is the one piece
  of text an extraction invents, and the only name a facade can collide on — every other name in the
  module was already unique, and an extraction only moves names *out*. A `mod report {` written beside
  an existing `pub mod report;` is `E0428` and the assist writes it without complaint, so the run would
  report success against a crate that no longer compiles. The check is lexical: it reads `mod`
  declarations and `use` bindings outside the seam, and a declaration *inside* the seam does not count
  because it moves into the new module and vacates the name. A module already extracted to its own file
  is out of reach of a lexical read; only same-file bindings are seen.
- **An extraction refuses a seam that would separate an attribute's string path from the item it
  names.** `#[serde(default = "default_extend")]` reaches an item the way a call does, but
  `textDocument/references` on the helper does not report that site — serde builds the call from the
  string's *contents*, so the identifier it generates has no span in the source. The stranded check is
  therefore blind to it, the visibility pass narrows the helper back to private, and the failure lands
  inside generated code. Only an unqualified name is weighed: a qualified one resolves the same from
  either side of the seam. `doc` attributes are excluded — their strings are prose.
- **`extract_module_to_file` names the new file itself**, after the module. There is no `to` field.
- **`extract_module`'s selection must cover whole items including their doc comments.** A doc
  comment belongs to the item below it, so a selection starting at `pub fn` leaves that item out.
- **`inline_method` rewrites callers in every file, and removes the definition only once it has
  rewritten them all.** A call sitting inside a macro invocation — `format!("{}", scaled(x))` — is
  one rust-analyzer will not rewrite, and the inline then leaves both that caller and the definition
  in place.
- **`extract_method` with `variant: "module"` is refused on a range that reads `this`.** TypeScript
  omits the module scope for such a range rather than reporting it inapplicable, so the operation
  refuses instead of quietly extracting a method.
- **Extraction placeholders are renamed by the engine**, not by string replacement — and the rename
  is *verified* rather than assumed. It has a blind spot: rust-analyzer rewrites references to the
  items it moved as `modname::Item`, and a reference already sitting inside another extracted module
  cannot be renamed, because from that scope `modname::Item` never resolved and rust-analyzer does not
  rename an unresolved path. That once produced nine such sites and fifteen compile errors from an
  operation that reported success. The operation now refuses when the placeholder occurs more often
  than it did before the assist ran, naming the lines.

  The refusal names **which of two causes** it hit, because they want opposite advice. A leftover
  inside an already-extracted *module* is an ordering mistake, and the rule that avoids it entirely is:
  **extract a definition before anything that references it.** Line order and dependency order are
  different axes and dependency order wins — build a small DAG of which seams define symbols other
  seams use, topologically sort it definitions-first, and use line order only to break ties.

  A module the file **already had** — its `mod tests` — reads the same lexically and is not an
  ordering mistake either. The one such leftover known is repaired before the check runs: in
  `mod tests { use super::base; … base() }` the assist repoints the import to
  `use super::modname::base;` *and* rewrites the call to `modname::base()`, which names nothing from
  inside `tests`. The backend puts the call back where the placeholder starts its path, the reference
  sits in a module other than the placeholder's, and that module's own `use` binds the moved name.
  With `use super::*;` the rewritten call resolves through the glob and the rename finishes it, so it
  is left alone. Any other leftover in such a module is refused with wording that names this case:
  reach the item through `use super::*;`, or cut the seam where that module does not name it.

  A leftover inside an **`impl`** is not an ordering mistake and reordering cannot fix it. One real
  split was reordered in full and produced byte-identical refusals at identical offsets, because the
  reference was merely *in the file* rather than inside an already-extracted module. Grow the seam
  instead. The check that predicts this before any index is paid is `restructure check`.

### `extract_class` is hand-written

Neither engine has an extract-class or a move-member refactor, so this one operation is performed by
the sidecar itself. The plan still carries only intent and the parser still rejects code-bearing
fields; what is relaxed is that the result is authored by this package rather than by TypeScript.

- The anchor must cover **whole class members**. A partially covered member is refused.
- The state the moved members read is bound through a constructor parameter, and the original class
  is left holding an instance and delegating to it.
- Members whose meaning would change on the way out are **refused by name**: a `super` call, a
  `#private` read, a write through `this`, a call to a member staying behind, an accessor, a static,
  a decorated member, an overload, a destructured parameter.
- Prefer the engine-backed route where it applies: for `this`-free members, `extract_method` with
  `variant: "module"` followed by `move_symbol` yields a module of free functions instead of a class
  of statics, and every character of it comes from TypeScript.
- Indentation of relocated bodies lands wrong; `yarn lint:fix` corrects it. There is no Rust
  equivalent of that step — a crate without a `rustfmt.toml` is usually not rustfmt-formatted, and
  running rustfmt would restyle code beyond the move and destroy the diff's reviewability. Rust
  extractions have not been observed to need it.

## What a plan may not contain

`create_file`, `insert_text`, `delete_range`, or any `text` / `code` / `content` field. The parser
rejects all of them, `extract_class` included. This is the guarantee that no hand-written code
enters a restructure *through a plan* — distinct from, and stronger than, the claim that every
character of a result comes from an engine, which `extract_class` alone does not meet.
