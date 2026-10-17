# Changeset: index-daemon-language-server-config

## Problem

An installed Tddy Desktop answered every hover with `[unavailable] lsp server exited`. The app's
`tddy-index-daemon` launched a bare `rust-analyzer`, and an app launched from the Dock has no
developer shell on its `PATH`: no `rust-analyzer`, no `cargo`, no `rustc`. The program was never
started, but the failed spawn dropped the registry's oneshot, which `LspRegistry` reads as
`ServerExited`. So a host that was not set up got reported as a server that crashed, as `Unavailable`
(try again later) instead of `FailedPrecondition`.

## State B

- **`index_daemon:` config** (`IndexDaemonConfig`) takes two more optional keys:
  - `rust_analyzer_path` is passed to the index daemon as `--rust-analyzer <path>`. When unset, the
    index daemon runs `rust-analyzer` from its own `PATH`, as before.
  - `env` is a map of variables set on the index daemon process (on top of the daemon's own
    environment). rust-analyzer, and the `cargo` / `rustc` it runs, inherit it. Toolchain lookup is
    expressed through it (`PATH`, `CARGO`, `RUSTC`, `RUST_SRC_PATH`, `RUSTUP_TOOLCHAIN`) rather than
    through dedicated keys.
- **`tddy-index-daemon --rust-analyzer <PATH>`** names the program its `LspAllowList` launches.
- **`tddy-lsp`**: a spawn that fails with `NotFound` is refused as
  `LspError::ServerNotFound("<program>: <io error>")`. Any other spawn failure is refused as
  `LspError::Io`. Both are sent back over `LspServerBody::client_tx`, which now carries
  `Result<Arc<LspClient>, LspError>`. `ServerExited` is reserved for a server that started and then
  went away.
- **`desktop.yaml.production`** documents the section, commented out, since nothing ships either
  binary.

## Deferred

- One environment for every workspace root. A per-project toolchain (a checkout's own nix/direnv
  shell) cannot be expressed; recorded as a TODO on `IndexDaemonConfig::env`.

## Package docs to update at wrap

- `packages/tddy-index-daemon/docs/code-index-service.md`: the `--rust-analyzer` flag.
- `packages/tddy-daemon/docs/`: the `index_daemon:` section's `rust_analyzer_path` and `env`.
- `packages/tddy-lsp/docs/`: spawn failures are `ServerNotFound` / `Io`, not `ServerExited`.

## Files

- `packages/tddy-daemon-kernel/src/config.rs`
- `packages/tddy-daemon/src/index_daemon/{spawn,registry}.rs`, `packages/tddy-daemon/src/index_daemon_body.rs`
- `packages/tddy-index-daemon/src/{cli,main}.rs`
- `packages/tddy-lsp/src/{server_body,registry}.rs`
- `desktop.yaml.production`
- Tests: `packages/tddy-lsp/tests/spawn_observer_test.rs`,
  `packages/tddy-index-daemon/tests/dual_transport_acceptance.rs`,
  `packages/tddy-daemon/tests/index_daemon_lifecycle_acceptance.rs` (plus the spawn-plan literals in
  `code_index_warmup_acceptance.rs`, `plan_dialog_acceptance.rs`, `code_navigation_acceptance.rs`)
