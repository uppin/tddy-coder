# 2026-10-22 — CLI argv from the harness registry

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `build_claude_argv` / `build_cursor_argv` delegate to the registry's descriptors (byte-identical,
`tests/cli_harness_parity.rs`); `launch_cli_harness` spawns any registered harness with a route's
extra arguments and environment. See [module layout](../module-layout.md).
