# 2026-09-09 — `move_module_to_crate`, and a rename that reaches other files

**Type:** Feature

Root node of the `#unbundle` stack ([#470](https://github.com/uppin/tddy-coder/pull/470)). Full
story in the cross-package entry: [2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

**A live defect fixed first.** `edits_for` filtered rust-analyzer's `documentChanges` down to the
anchor's own URI, so a `rename_symbol` of a symbol referenced elsewhere **threw every other file's
edits away** and broke those callers silently. `workspace_edits_for` is the multi-document primitive
that replaces it, and `rename_symbol` now routes through it. Without this fix nothing could re-point
a caller, so nothing could move across a crate.

**The eighth operation.** `move_module_to_crate` (`crate_move.rs`) emits `FileEdit::Rename` — which
`apply.rs` already performs with `git mv`, so history survives — rewrites the moved file's own
`use crate::…` / `use super::…` header, re-points every caller from a real
`textDocument/references` result, and edits both `Cargo.toml`s. `reexport: "glob"` leaves
`pub use <crate>::*;` in the origin for a zero-caller-diff move; `"named"` is **refused**, because a
named re-export puts items at the destination's crate root while a caller writes
`crate::<module>::Item`.

**`survey` and `resolve` take an engine seam, not the backend.** Their red-phase signature
`(&Workspace, &RefactorOp)` could not reach `textDocument/references` at all, so `callers` could only
ever have come back empty. Taking `&mut RustBackend` instead would have made `crate_move` and
`backends/rust.rs` mutually dependent, and making them `RustBackend` methods would put a
language-agnostic transformation — a `git mv`, two manifest edits, a `pub use` line — inside the Rust
engine driver and make it untestable without a live server. `ModuleReferences` is the one row only a
server can answer, so that is where the seam is cut.

**Every acceptance test ends in `cargo check`.** The operation's first live run emitted a tree that
read correctly and did not compile, on **both** the facade and the no-facade path, while all 271 unit
tests passed — a manifest that is never compiled looks fine. `move_module_to_crate_acceptance.rs` and
`rename_cross_file_acceptance.rs` drive a live rust-analyzer over a real three-crate fixture, run in
the CI gate (~13.5s), are **not** `#[ignore]`d, and fail rather than skip when rust-analyzer is
absent. Serialised two ways: a `rust-analyzer` test group in `.config/nextest.toml` across processes,
and a lock in the harness within one binary, since plain `cargo test` never reads that file.

**`check --budget LINES`** reports which of the files a plan's **anchors** name exceed the budget —
a report, never a gate. It cannot see a file no plan anchors, which is why the operation's own
1,609-line `crate_move.rs` is absent from its output.

251 → 287 tests. Limitations found on the first real use are in
[the feature doc](../../../../docs/ft/coder/rust-code-restructuring.md#known-limitations); the
operational debt behind them is in
[`docs/dev/todo/`](../../../../docs/dev/todo/2026-09-09-restructure-defects-from-the-first-cross-crate-move.md).
