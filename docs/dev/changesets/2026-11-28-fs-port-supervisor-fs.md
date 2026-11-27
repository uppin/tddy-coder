# 2026-11-28 — Supervisor and spawn go through the Fs port

**Type:** Refactoring · **PR:** [#1052](https://github.com/uppin/tddy-coder/pull/1052) (`#fs-port` 8/13)
**Packages:** `tddy-supervisor`, `tddy-spawn`
**Base:** `feature/fs-port/agents-fs` at `92d92bb78` (n7, itself on n6/n5/master)

The production disk I/O of the supervisor and spawn crates goes through the `Fs` port. The
supervisor, cgroup broker and config loaders hold injected handles (`Supervisor.fs`,
`CgroupBroker` fs), and spawner entry points delegate to `<fn>_on` seams, with `DiskFs` only
at composition roots: the supervisor's construction, daemon composition sites, and
backward-compat wrappers kept for callers outside the migrated crates. Every crate carries
`tests/fs_port.rs` with the `direct_disk_io_in` guard over its own `src/`. Detail: [the
filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).

No port surface was added on this node. Known gaps (all recorded here rather than silently
left): descriptor-bound operations stay on `libc`/`OwnedFd` — the host-socket directory
descriptor (`O_NOFOLLOW` open, `fstat`, `fchown`, `fchmod`), child stdio fds, `fchown`/`lchown`
ownership hand-offs, and the spawn worker's pipe ends (a dedicated `PipeEnd` type over
`libc::read`/`write`). `/proc` reads go through `DiskFs` (the host's process table, not
supervisor disk). Local verification note: this machine's checkout ancestors are
group-writable, so the supervisor's exec gate refuses every spawn under this checkout — the
policy and spawn integration suites fail here with `Denied` on the unmodified base too
(verified by stashing this node's changes); CI's clean checkout is the authority for those
suites. Test-only crates keep real disk.
