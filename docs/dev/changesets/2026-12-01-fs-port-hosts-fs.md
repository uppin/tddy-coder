# 2026-12-01 — Hosts, VMs, git and projects go through the Fs port

**Type:** Refactoring · **PR:** [#1056](https://github.com/uppin/tddy-coder/pull/1056) (`#fs-port` 12/13)
**Packages:** `tddy-vm`, `tddy-host-service`, `tddy-git`, `tddy-projects`, `tddy-discovery`, `tddy-github`, `tddy-git-clone`, `tddy-credential-sync`
**Base:** `feature/fs-port/code-tools-fs` (n11)

The production disk I/O of the host/VM/git crates goes through the `Fs` port. The VM
library and manager, the host stores (registry, keypair, desktop targets), the project
registry and the discovery executors delegate to `<fn>_on` seams or held handles
(`VmLibrary.fs`, `VmManager.fs`, the stores' `with_fs`, `CodebaseAccess::Local(Arc<dyn
Fs>)`, `ToolExecutor::Local(Arc<dyn Fs>)`), the async build/cloud-init pipelines take an
`AsyncFs`, with `DiskFs` only at composition roots: old-name wrappers, binary `main`s,
front-end constructors and test builders. Every crate carries `tests/fs_port.rs` with
the `direct_disk_io_in` guard over its own `src/`. Detail:
[the filesystem port](../../../packages/tddy-fs/docs/filesystem-port.md).

No `Fs` trait method was added on this node. Known gaps (recorded here rather than
silently left): descriptor acts stay on `libc` — the `/dev/kvm` probe, the emulator
stderr log's `O_NOFOLLOW` open and read-back, and the streamed reads of multi-hundred-MB
images (headers, whole-image comparisons), for which the port has no ranged/streaming
read; that op is port surface for n1. GitHub REST temp files are written for `curl` to
read by path and go through the disk port only. The UEFI vars store is materialised as
its zero bytes because the port has no truncate. A `tddy-github` shape test now exempts
`tddy-fs` alongside `tddy-remote-shell` as a leaf port `tddy-git` may depend on.
Test-only crates keep real disk.
