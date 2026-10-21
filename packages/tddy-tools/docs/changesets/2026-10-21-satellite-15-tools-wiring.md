# 2026-10-21 — tddy-tools is a library; tddy-tools-cli builds the binary

**Type:** Architecture

`#satellite` 15/15 ([#690](https://github.com/uppin/tddy-coder/pull/690)); cross-package entry:
[2026-10-21-satellite-15-tools-wiring.md](../../../../docs/dev/changesets/2026-10-21-satellite-15-tools-wiring.md).

- `[[bin]] tddy-tools`, the optional `tddy-livekit` dependency and `[features] default = ["livekit"]`
  are gone, with `env_logger`, `tddy-bsp`, `tddy-code-analysis` and `libc` (only `main.rs` used them).
  `execute-tool-stdio-fixture` stays.
- `cli`, `index_client`, `index_console`, `pty_relay`, `remote_cli` and `session_hook` are public
  modules of the library (they were declared by `main.rs` alone); `tddy_tools::` paths inside them
  became `crate::`, and `index_client::run_restructure` widened from `pub(crate)` to `pub`.
- 35 binary suites moved to `tddy-tools-cli/tests/` (renames); 19 library suites stay. Scoped run:
  `tddy-tools` 266 passed, `tddy-tools-cli` 160 passed, 1 ignored (426 before the move, all in
  `tddy-tools`).
- Code issues: `complexity-cli-run-call-tool` unchanged (93 lines, `cli.rs:525`);
  `oversized-file-cli` recorded (895 production lines, not grown).
