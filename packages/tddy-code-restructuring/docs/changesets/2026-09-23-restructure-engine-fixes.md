# 2026-09-23 — The engine stops reporting success over results it cannot vouch for

**Type:** Fix · `#carve` 13/15, PR [#527](https://github.com/uppin/tddy-coder/pull/527)
Cross-package entry: [`docs/dev/changesets/2026-09-23-restructure-engine-fixes.md`](../../../../docs/dev/changesets/2026-09-23-restructure-engine-fixes.md)

Import pass (E1, grouped `use`, gap A): `names_bound` reads what a `use` binds (the alias, nothing for `as _`), both parent reconstructions are verified and refused by name when they make no progress, only the names the seam lost are weighed, a grouped binding the assist removed still decides the import, and a reconstructed relative path is rebased for the child module. `impl` seams (E3, gap B): a cut through an inherent `impl` is no longer refused, and the assist's `self.modname::f()`, `Self::modname::f` and `Type::modname::f` rewrites are undone; trait-`impl` cuts stay refused. Nested modules (gap C): a call the assist rewrote inside a module the file already had is put back. See [assist-output-repairs.md](../assist-output-repairs.md).

Readiness waits for `experimental/serverStatus` quiescence (E2's fixture cause), a degraded index (`health` not `ok`, `warning` included) is refused as `ServerDefect`, an `extract_method` range that returns from its enclosing function is refused as `SeamRefused` (E4), and every writing `apply` is bracketed by `cargo check --all-targets` (`BaselineDoesNotCompile`, `AppliedTreeDoesNotCompile`; cancellable, no opt-out). See [readiness-and-gates.md](../readiness-and-gates.md).

New modules: `backends/rust/{imports,impl_seam,chatter,readiness,early_return,nested_modules}.rs`, `runner/compile_gate.rs`; `ServerChatter` is re-exported at its old path. `runner::open_run_after` takes a `before_writing` gate. Code issues: `oversized-file-backends-rust` narrowed 4,788 → 4,316 and kept; `oversized-file-test-binary` 966 unchanged; `complexity-rust-facade-lines` 47 unchanged, moved to `rust.rs:3522`.
