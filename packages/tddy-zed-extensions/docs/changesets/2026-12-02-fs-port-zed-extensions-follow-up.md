# 2026-12-02 — tddy-zed-extensions goes through the Fs port

**Type:** Refactoring · PR: the fs-port follow-up

This crate's production disk I/O goes through `tddy-fs` behind injected handles
(`GrammarStore`, `ExtensionSupport`, `OutlineBackend`, `WasmHost`, `HostState`,
`Staging`) and `<fn>_on` seams (`Index`, `ExtensionSettings`, `ExtensionLanguage`,
`install`, `staging`, `archive`, the WASI preopen's worktree reads); `DiskFs` stays
at composition roots and old-name wrappers. The WASI bindings' `fs` alias is now
`wasi_fs`, so `tests/fs_port.rs`'s `direct_disk_io_in` guard reads the crate clean.
Cross-package entry:
[`docs/dev/changesets/2026-12-02-fs-port-zed-extensions-follow-up.md`](../../../../docs/dev/changesets/2026-12-02-fs-port-zed-extensions-follow-up.md).
