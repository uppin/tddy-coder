# 2026-10-22 — cli.rs split into session_tools, changeset_commands and relay

**Type:** Refactor

`#satellite` 16/16, the sweep ([#695](https://github.com/uppin/tddy-coder/pull/695)); cross-package entry:
[2026-10-22-satellite-16-sweep.md](../../../../docs/dev/changesets/2026-10-22-satellite-16-sweep.md). Behaviour-preserving unless a line says otherwise; production lines are counted to the first
`#[cfg(test)]`.

- `src/cli.rs` **895 → 459**: `cli/session_tools.rs` (174) and `cli/changeset_commands.rs` (78) by
  `move_item`; the five `#[cfg(unix)]` / `#[cfg(not(unix))]` relay families to `cli/relay.rs` (231) by
  hand with the developer's consent (the engine cannot name a `cfg` twin), bodies byte-identical,
  `TODO(restructure-engine)` at its head.
- Record closed and deleted: `oversized-file-cli.md` (895 → 459). `complexity-cli-run-call-tool.md`
  moved to `cli/session_tools.rs`, unchanged at 93 lines.
