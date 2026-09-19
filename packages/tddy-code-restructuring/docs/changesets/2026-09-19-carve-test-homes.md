# 2026-09-19 — `move_test_binary_to_crate`, and a facade walk that actually walks

**Type:** Feature

`#carve` 4/10 ([#498](https://github.com/uppin/tddy-coder/pull/498)). Cross-package entry:
[2026-09-19-carve-test-homes.md](../../../../docs/dev/changesets/2026-09-19-carve-test-homes.md).

**The tenth operation.** `move_test_binary_to_crate` (`crate_move/test_binary.rs`) takes an anchor
of `<crate>/tests/<name>.rs` and emits three edits: the `git mv`, the moved file's own text, and the
destination's `[dev-dependencies]`. There is **no origin edit at all** — cargo auto-discovers
`tests/*.rs`, so the crate the test left never named it — and `reexport` is refused rather than
ignored, because nothing can reference a test binary and a facade would keep nothing resolving.
`TestBinaryMove` is deliberately not `Move`: that struct carries `module`, `origin` and `reexport`,
and a test binary has none of the three. The surface and its refusals are in
[test-binary-moves.md](../test-binary-moves.md).

**`defining_crate` could not resolve either facade in this workspace, and the fix is in
`module_home`.** This was an authorised departure from the node's boundary contract — nodes 5–10
build on that symbol. Two defects: `re_export_target` matched within a single line, and every facade
here is a multi-line braced group whose crate-naming line names no member, so a group of forty
re-exports read as a re-export of nothing; and `defining_module_in_crate` read only the crate root,
so a `pub mod config;` whose file is nothing but `pub use <other>::config::*;` read as locally
defined. Only whole-file forwarding counts — a file that forwards *and* declares something of its
own is a module of this crate, and over-resolving is the worse failure. `module_home()` and
`defining_crate()` keep their signatures; every change is in private helpers and every existing test
stayed green.

**Re-pointing reads the whole file, and so does the dependency question.** A moved module keeps its
`crate::`, so its bodies are left alone; a test binary leaves its crate entirely, so every
occurrence of the origin's extern name is re-pointed — 110 in bodies, plus one `use` indented inside
a `mod tests { … }`, survived a header-only pass. Reading the header alone for dependencies was the
same mistake from the other side: one suite named `tddy_github` in a return type, `tddy_connectrpc`
in a `let` and `axum` in a statement while declaring none of them, and the moved file failed with
`E0433`. A name counts as a crate only when it opens a path, is not bound by the file, is not
`crate`/`self`/`super`/`std`/`core`/`alloc` or the origin, and **is declared by the origin's own
manifest** — which is what makes a name with no line to carry across a refusal rather than a guess.

**One span pass decides what text a path may be read out of.** Code is re-pointed and counted;
comments are re-pointed and not counted, since prose naming a crate the file no longer uses is the
debt this operation pays off; string literals are left entirely alone, because what is in one is
data the suite asserts on. The scanner handles raw strings of any hash depth and distinguishes a
character literal from a lifetime. The cost is stated in the limitations: a
`CARGO_MANIFEST_DIR`-relative string path to a sibling crate's source is a hand edit after the move.

25 new tests (18 in `tests/test_binary_move.rs`, 3 inline in `module_home.rs`, 4 in the consuming
crate). `check` has no static preflight for this operation yet — the marker is at `plan.rs:1738`.

**Deferred with consent:** `crate_move/test_binary.rs` is **966** production lines, 1.9× the budget,
created by this node. Records: [`oversized-file-test-binary.md`](../code-issues/oversized-file-test-binary.md)
and `docs/dev/todo/2026-09-19-test-binary-rs-is-950-production-lines.md`. `backends/rust.rs` gained
16 lines (4,772 → 4,788) and is not split here because #491 is in flight over the same file.
