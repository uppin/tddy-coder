# 2026-11-30 — The code tools go through the Fs port

**Type:** Refactoring · **PR:** [#1055](https://github.com/uppin/tddy-coder/pull/1055) (`#fs-port` 11/13)
**Packages:** `tddy-code-restructuring`, `tddy-code-analysis`, `tddy-index-daemon`, `tddy-lsp`, `tddy-lsp-executor`, `tddy-semantic-index`, `tddy-bsp`
**Base:** `feature/fs-port/workflow-fs` (n10)

The production disk I/O of the code-tool crates goes through the `Fs` port. The
restructure engine's run state (plan store, journal, ledger, pre-images, spawn records),
its file budget/tidy/compile gates and the coverage pipeline delegate to `<fn>_on` seams
or held handles (`Options.fs`, `Workspace.fs`, `PlanStore`, `WorkspaceIndex`, the LSP
executors), with `DiskFs` only at composition roots: old-name wrappers, front-end
defaults (`Options::default`, the hook/executor constructors) and test builders. Every
crate carries `tests/fs_port.rs` with the `direct_disk_io_in` guard over its own `src/`.
Detail: [the filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).

No `Fs` trait method was added on this node. Known gaps (recorded here rather than
silently left): `tddy-bsp` has no production disk I/O in `src/` at all (its guard was
already green — nothing to port); the journal's append now rides `Fs::append` (a
`write_all`), which drops the fsync the hand-rolled append did before returning — a
durable-append operation is port surface for n1 to add, recorded here; spawn records now
append per line through the port instead of holding a `File` open; the coverage
capture's `cargo`/`llvm-cov` subprocess file effects stay process acts, with the rustc
wrapper script written through the port (mode included); the local embedding model's
hub download stays the hub client's, its cache directory and config read through the
port. Test-only crates keep real disk.
