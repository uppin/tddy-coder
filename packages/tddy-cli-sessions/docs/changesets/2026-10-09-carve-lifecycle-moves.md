# 2026-10-09 — New crate: the CLI session PTY runtime

**Type:** Architecture

Created by `#carve` 21/21 ([#536](https://github.com/uppin/tddy-coder/pull/536)): `CliSessionManager` with its
PTY, control-lease and terminal modules, and `session_toolcall`. 1,738 production lines in 12 modules, 9 tests.
It depends on neither the split, launch nor agents crates. Layout: [module-layout.md](../module-layout.md).
