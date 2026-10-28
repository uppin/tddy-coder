# 2026-10-28 — ReadSessionTerminal

**Type:** Feature

`#home-inbox` 2/8 ([#767](https://github.com/uppin/tddy-coder/pull/767)). The project-home agent
reads a session's main terminal as text. Across:

- `tddy-task` — `TerminalCapture::text_grid() -> Option<TextGrid>`, `TextGrid { lines, cursor:
  GridCursor { row, col, visible }, cols, rows, offset, bracketed_paste }`: the active buffer
  (`Vt::view()`, not `Vt::text()`, which is the primary buffer), rows `trim_end`ed, one borrow so it
  is one moment; `None` until the model is sized. A separate accessor, not a change to
  `ScreenSnapshot`, so `#term-snapshot`'s symbols are untouched. 14 unit tests over real escape
  sequences (`tests/terminal_capture_text_grid.rs`).
- `tddy-session-lifecycle` — `DaemonSessionHost::terminal_sessions()` is public (was
  `pub(crate) terminal_store`): the project-home reader resolves `main` through the store the
  terminal RPCs use, sandbox first.
- `tddy-tool-engine` — new `project_home_terminal_tools.rs`: `READ_SESSION_TERMINAL`,
  `ProjectHomeTerminalPort` (a supertrait of `ProjectHomePort`, so one `&dyn` resolves the target and
  reads), `execute_project_home_terminal_tool`. A target tool (`daemon_instance_id`), so another
  host's session is forwarded whole by `#start-parity` 1/9's generic forward. The answer omits
  `bracketed_paste`, which `#home-inbox` 5/8 reads. Registered in the catalogue, `is_project_home_tool`,
  `is_project_home_target_tool` and the read-only pin.
- `tddy-daemon-rpc` — `DaemonProjectHome` implements the port over the terminal store
  (`get_terminal(session, "main")`, capture lock, `text_grid`); new edges to `tddy-terminal-rpc` and
  `tddy-task`. The test support's `Host::terminals` lets a test start a session's child in the
  daemon's terminal manager.
- `tddy-tools` — `ReadSessionTerminal` in the admitted list. `tddy-agent-launch` — the project-home
  prompt names it.

Decisions: answer field names are `lines` / `rows`; `TextGrid` sizes are `u16` like `ScreenSnapshot`'s;
registration and wiring stayed out of the red commit so the acceptance tests failed for the missing
implementation, not a missing list entry.

Code issues: `terminal_capture.rs` 668 → 728, `project_home_tools.rs` 699 → 709 and
`exec_tool/project_home.rs` 641 → 686 production lines. The splits are deferred — the three files are
shared with `#home-inbox` 5/8 and 6/8 — and each record carries the new measurement.
