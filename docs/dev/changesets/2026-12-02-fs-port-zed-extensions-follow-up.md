# 2026-12-02 — tddy-zed-extensions goes through the Fs port; sandbox-app call sites fixed

**Type:** Refactoring · PR: the fs-port follow-up
**Packages:** `tddy-zed-extensions`, `tddy-sandbox-app`
**Base:** `master`

`tddy-zed-extensions` — the one gap #1057 left — now routes its production disk I/O
through the `Fs` port: `GrammarStore`, `ExtensionSupport` and `OutlineBackend` hold
injected handles; `Index`, `ExtensionSettings`, `ExtensionLanguage`, `install`,
`staging`, `archive` and the WASM host's file reads run behind `<fn>_on` seams;
`DiskFs` stays at the composition roots (the CLI's `main`s, the extension-host
wiring, the tests). The WASI bindings' `fs` alias is renamed `wasi_fs`, so the
`direct_disk_io_in` guard no longer mistakes the interface's own `fs::` namespace
for disk I/O. The guard is green: the crate scans clean.

`tddy-sandbox-app`'s four call sites that survived #1057's merge compile against the
port's `&Path` signatures again (`create_dir_all`, `canonicalize`, `write_atomic`).

Closes `docs/dev/todo/2026-12-02-fs-port-zed-extensions-follow-up.md` — final
measurement: the guard's `direct_disk_io_in` scan of the crate's `src/` reports 0
hits (was 97 across 12 files, 34 of them the WASI bindings' `fs::` namespace).
