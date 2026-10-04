# 2026-10-04 — The executor behind a session's `Lsp*` tools follows `index_daemon:`

**Type:** Feature

`#live-plan` 11/15, PR [#570](https://github.com/uppin/tddy-coder/pull/570). Cross-package entry:
[2026-10-04-session-lsp-tools.md](../../../../docs/dev/changesets/2026-10-04-session-lsp-tools.md).

`src/index_daemon/lsp_channel.rs` implements `IndexChannel` for `IndexDaemonRegistry`, delegating to
`connect` — the call the code pane's navigation already forwards through. `runtime.rs` builds one
`TddyLspExecutor`, takes its registry for the idle-reaper loop, and registers
`select_lsp_executor(...)`'s result: the index-backed executor with an `index_daemon:` section, the
registry-backed one without. It no longer calls `tddy_lsp_executor::register`, which builds its own
executor and would leave a second, unused one behind. See
[daemon-endpoint.md](../daemon-endpoint.md).

`tests/code_navigation_acceptance.rs`'s fake index gains the two new methods of the extended trait, each
answering `not_part_of_this_fake`.

Code issues: `runtime.rs` grew 1,636 → 1,650 production lines (`oversized-file-runtime`) and `build` 891 →
905 (`complexity-runtime-build`); the split is deferred with the developer's consent.
