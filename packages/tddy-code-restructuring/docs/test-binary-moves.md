# `move_test_binary_to_crate`

Moves `<crate>/tests/<name>.rs` into the crate whose code it exercises. `to` is the destination
crate's directory and is required; `reexport` is **refused**.

A test binary is a different shape from a module, and admitting the wrong path shape would produce
an edit neither operation's rules cover — which is why this is a separate operation rather than a
loosening of `move_module_to_crate`.

| | Module | Test binary |
|---|---|---|
| Path | `<crate>/src/<module>.rs` | `<crate>/tests/<name>.rs` |
| Declaration | a `mod` line to find and remove | **none — cargo auto-discovers `tests/*.rs`** |
| Callers | reachable from anywhere; may need a facade | **nothing can reference a test binary** |
| Manifest | destination `[dependencies]` | destination `[dev-dependencies]` |
| Cycle back to the origin | refused | permitted — a dev-dependency does not enter the library's build |

So the move is three edits and no more: the `git mv`, the moved file's own text, and the
destination's `[dev-dependencies]`. **There is no origin edit at all** — the crate the test left
never named it and has nothing to stop naming.

Only `<crate>/tests/<name>.rs` is a test binary. `tests/common/mod.rs` is a module of whichever
binaries declare it, so moving one on its own would take a module out of every binary that names
it; the anchor shape is read from the string before any manifest is opened, so the refusal can name
the shape it wanted.

## Re-pointing: the whole file, through every facade

This is where the operation differs from a module move in substance rather than in shape.

**Only paths that open with the origin's extern name change.** A test binary already *is* its own
crate root, so `crate::` and `super::` name the binary — the `mod common;` it declares, not the
library beside it — and mean exactly the same thing in the destination's `tests/`. That is the
opposite of a module move, where `crate::` is the whole of the header pass. A path whose first
segment is a module the file itself declares is left alone for the same reason: in Rust 2018 an
unqualified first segment resolves to a crate-root item before it resolves to an extern crate.

**Every occurrence is re-pointed, not just the leading `use` header.** A moved module keeps its own
`crate::`, so a path in a function body means the same thing afterwards. A test binary takes the
whole file out, and `tddy_daemon::project_storage::add_project(…)` in a body is an extern-crate path
naming a crate the destination need not depend on at all. 110 such paths, and one `use` indented
inside a `mod tests { … }`, survived a pass that read the header alone.

**Each path is resolved to the crate that *defines* what it reaches**, through
[`module_home::defining_crate`](../src/crate_move/module_home.rs) applied repeatedly. A test was
written against whatever path compiled at the time, which in this workspace can be two facades deep:
`tddy_daemon::host_registry` is `tddy-session-lifecycle`'s re-export of `tddy-host-service`'s
module. Stopping at the first hop yields a file that compiles and still names the wrong crate. The
walk stops at a crate that defines the module itself, at one this workspace does not reach by a path
dependency (a published crate's re-export is not ours to follow), and at one already visited, which
a pair of crates re-exporting each other would otherwise loop through.

A path whose second segment is a group or a glob — `tddy_daemon::{a, b}` — is **refused**. Which
crate defines a group has as many answers as the group has members and they need not agree;
splitting the declaration is the plan author's call. A bare `use tddy_daemon;` names no module, so
there is nothing to resolve.

### What `defining_crate` has to see to answer this

Two shapes in this workspace defeat a naive reading of a crate root, and both are resolved:

- **A re-export is a declaration, not a line.** Every facade here is a braced group spread over
  several lines, and the line naming the crate — `pub use tddy_session_lifecycle::{` — names no
  member at all. Matched a line at a time, a group of forty re-exports looks like a re-export of
  nothing, and every module in it is reported as defined by the crate that merely passes it on.
- **A `mod` line in the crate root is not proof that the crate defines anything.** A module file
  that is a doc comment and `pub use <other_crate>::<module>::*;` and nothing else is a forwarding
  address; reading the root alone answers "this crate defines it", and a caller re-pointed on that
  answer goes on naming the crate the carving was supposed to take it out of. Only the whole-file
  shape counts — a file that forwards *and* declares something of its own is a module of this crate
  with a re-export in it, and calling the other crate its home would send a dependency to a crate
  holding half of what the caller names.

Over-resolving is the worse failure of the two, which is why a partial re-export — a group, or a
single item — is read as a local definition.

## What the destination's `[dev-dependencies]` gain

`[dev-dependencies]`, not `[dependencies]`: the file lands in the destination's `tests/`, and a
crate only a test needs is not one the library needs.

The crates to declare are collected from the same text the re-pointing pass reads, and from the
whole of it — reading the header alone left a moved suite naming `tddy_github` in a return type,
`tddy_connectrpc` in a `let` and `axum` in a statement with a line for none of the three, and
`E0433`. The head of a path is not self-evidently a crate, so a name is taken as one only when all
of these hold:

- it is the **first** segment of a path, and a segment inside a `use` group is not one;
- it is not a name the file binds — a module it declares at any depth, or anything a `use`
  declaration brings into scope, aliases included (`use tokio::sync::mpsc;` makes every later
  `mpsc::…` an imported module);
- it is not `crate`, `self`, `super`, `std`, `core`, `alloc`, or the origin;
- **the origin's manifest declares it.** A test compiles in the crate it sits in today, so every
  crate it names is declared there; a head declared nowhere is not a crate at all, whatever it looks
  like.

A crate the destination already declares in either table is left alone, and so is the destination
itself. A crate the facade walk placed in this workspace has its `path` line **authored**, from the
destination back to it — a fact about this repository's own layout. A crate the walk could not place
is **copied** from the manifest that already declares it, because a version invented here would be a
fact about the world this operation has no way to know. A named crate with no line to copy is a
refusal: it means the manifest the plan read is not the one the test compiles against.

### Code, comments, strings

One pass decides what text a path may be read out of, and all three rules matter:

- **Code** is re-pointed and counted. A path there is resolved by the compiler, so leaving it naming
  the origin is a file that does not build in its new home.
- **Comments** are re-pointed and **not** counted. A sentence describing what a suite exercises is
  wrong the moment the suite exercises it from somewhere else; it is prose, not a declaration, so it
  is no reason for the destination to gain a dependency.
- **String literals are left entirely alone.** What is written in one is data the suite asserts on —
  an error message, a fixture, a path — produced by whatever emits it rather than resolved from this
  file's imports. Rewriting it would change what the test asserts.

The scanner reads every literal shape Rust has, including raw strings of any hash depth and
character literals; `'` opens a lifetime far more often than a literal, so it counts only when a
closing quote follows one character or one escape.

## Refusals

| Reads | Cause |
|---|---|
| `plan is malformed: …cannot write a facade…` | `reexport` on a test-binary move — nothing can reference a test binary |
| `plan is malformed: …needs \`to\`…` | no destination crate directory |
| the anchor is not `<crate>/tests/<name>.rs` | a module path, or `tests/common/mod.rs` |
| a path opening with the origin and continuing into a group or a glob | ambiguous defining crate; split the declaration |
| a named crate the origin's manifest declares in neither table | the plan read a manifest the test does not compile against |

## Limits

- **String paths to sibling sources are not rewritten.** A suite reading
  `concat!(env!("CARGO_MANIFEST_DIR"), "/../<crate>/src/<file>.rs")` still resolves from its new
  crate only by coincidence; string literals are deliberately untouched, so such a path is a hand
  edit after the move.
- **`check` has no static preflight for a test-binary move.** The plan's vocabulary refusals are
  reported, but the path shape and the facade walk are resolved at apply time.
- **The scanners read lines, not parsed items**, for `use` and `mod` declarations, so an unindented
  `use`/`mod` line inside a raw-string fixture is read as a binding. The failure mode is a
  dev-dependency line not written, which is a compile error at CI, never a silent wrong edit.
