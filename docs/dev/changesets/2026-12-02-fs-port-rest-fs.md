# 2026-12-02 — Every remaining production crate goes through the Fs port

**Type:** Refactoring · **PR:** [#1057](https://github.com/uppin/tddy-coder/pull/1057) (`#fs-port` 13/13)
**Packages:** `tddy-zed-extensions`, `tddy-coder`, `tddy-tools`, `tddy-presenter`, `tddy-telegram-control`, `tddy-toolcall`, `tddy-service`, `tddy-model-registry`, `tddy-tui`, `tddy-livekit`, `tddy-screen-sharing`, `tddy-graph`, `tddy-acp-stub`, `tddy-telegram`, `tddy-terminal-rpc`, `tddy-vm-build`, `tddy-tools-cli`, `tddy-session-sync-cli`, `tddy-livekit-screen-capture`, `tddy-coder-cli`, `tddy-work-queue`, `tddy-sandbox-qemu`, `tddy-build`
**Base:** `feature/fs-port/hosts-fs` (n12)

The production disk I/O of the remaining crates goes through the `Fs` port: held
handles on the big stateful pieces (`Presenter.fs`, the telegram harness, the model
registry store, `FileSessionStorage`, `ExecuteOptions.fs`), `<fn>_on` seams on the
file-format helpers, an `AsyncFs` where the code is async, and `DiskFs` only at
composition roots — old-name wrappers, binary `main`s and front-end constructors.
Every ported crate carries `tests/fs_port.rs` with the `direct_disk_io_in` guard over
its own `src/`. Detail: [the filesystem
port](../../../packages/tddy-fs/docs/filesystem-port.md).

No `Fs` trait method was added on this node. Known gaps (recorded here rather than
silently left): **`tddy-zed-extensions` is not ported** — 117 direct-disk-I/O lines in
`src/` remain and its guard is still red; the follow-up is recorded in
`docs/dev/todo/2026-12-02-fs-port-zed-extensions-follow-up.md`. The CLI binaries
(`tddy-tools`, `tddy-coder`, `tddy-tools-cli`) route through the port's `DiskFs` at
their main composition roots rather than held handles. Log/record sinks that used to
hold a `File` open now append per record through the port. Link creation in
`tddy-build` stays on `libc` (the port has no link operation). Test-only crates keep
real disk.
