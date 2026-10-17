# 2026-10-17 — The index daemon's toolchain is one environment for every workspace root

**Category:** Future enhancement
**Source:** [#649](https://github.com/uppin/tddy-coder/pull/649) (`index_daemon.env`)

`index_daemon.env` is set on the one `tddy-index-daemon` process, so every workspace root it serves loads with the same
`PATH`/`CARGO`/`RUSTC`. A checkout whose toolchain is its own nix or direnv shell — this repo's included — cannot be
expressed: the host config names one toolchain, and a `/nix/store` path copied from a dev shell disappears on `nix gc`.

Deferred by the developer (2026-10-17, "we'll solve it in future"). Candidate shapes: resolve the environment per root
(e.g. `nix print-dev-env` / `direnv export json` in the root, cached) and pass it as `LaunchSpec::env` per spawned
rust-analyzer, rather than on the index daemon process.
