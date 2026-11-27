# 2026-11-28 — The sandbox crates go through the Fs port

**Type:** Refactoring · **PR:** [#1053](https://github.com/uppin/tddy-coder/pull/1053) (`#fs-port` 9/13)
**Packages:** `tddy-sandbox`, `tddy-sandbox-app`, `tddy-sandbox-recipes`, `tddy-sandbox-runner`, `tddy-sandbox-cgroups`, `tddy-sandbox-darwin`
**Base:** `feature/fs-port/supervisor-fs` at `1db3ee65e` (n8, itself on n7/n6/n5/master)

The production disk I/O of the six sandbox crates goes through the `Fs` port. Materialize,
context walks, manifests, recipes, the cgroup broker and both binaries' session setup delegate
to `<fn>_on` seams or held handles (`APP_FS`, `RUNNER_FS`), with `DiskFs` only at composition
roots: binary `main`s, daemon composition sites, and backward-compat wrappers kept for callers
outside the migrated crates. Every crate carries `tests/fs_port.rs` with the
`direct_disk_io_in` guard over its own `src/`. Detail: [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md).

No port surface was added on this node. Known gaps (all recorded here rather than silently
left): link creation/reading and ownership/fd acts stay on `libc` (the port has no link or
descriptor operation); glob matching and Seatbelt profile rendering stay on their crates; the
cgroup smoke suite needs a delegated cgroup v2 subtree and is left to CI. Test-only crates
keep real disk.
