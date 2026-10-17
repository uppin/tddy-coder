# 2026-10-17 — The index daemon's rust-analyzer and toolchain are configuration

**Type:** Fix

([#649](https://github.com/uppin/tddy-coder/pull/649)) An installed Tddy Desktop answered every hover with `[unavailable] lsp server exited`.
Its `tddy-index-daemon` launched a bare `rust-analyzer`, and an app launched from the Dock has no developer shell on
its `PATH` — no `rust-analyzer`, `cargo` or `rustc`. The spawn failed, the dropped oneshot was read as
`ServerExited`, and a host that was not set up was reported as a crashed server (`Unavailable`).

- `index_daemon:` takes `rust_analyzer_path` (passed as `tddy-index-daemon --rust-analyzer <path>`) and `env` (set on
  the index daemon; rust-analyzer and the `cargo`/`rustc` it runs inherit it). Both optional.
- `tddy-lsp` refuses a spawn that fails as `ServerNotFound` (`NotFound`) or `Io`, sent over `LspServerBody::client_tx`
  (now `Result<Arc<LspClient>, LspError>`); `tddy-index-daemon` maps it to `FailedPrecondition`.
- `desktop.yaml.production` documents the section, commented out: nothing ships either binary.

Deferred: one environment for every workspace root; a per-project toolchain is
`docs/dev/todo/2026-10-17-index-daemon-toolchain-is-one-environment-for-every-root.md`.

`packages/tddy-daemon-kernel/src/config.rs` grew 1,542 → 1,557 production lines (its `oversized-file` record carries
the row).

Docs: [warm-code-intelligence-daemon.md](../../ft/coder/warm-code-intelligence-daemon.md#lifecycle),
[reusable-lsp.md](../../ft/coder/reusable-lsp.md).
