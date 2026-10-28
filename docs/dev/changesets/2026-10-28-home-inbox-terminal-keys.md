# 2026-10-28 — ClaimSessionTerminal and SendSessionKeys

**Type:** Feature

`#home-inbox` 5/8 ([#770](https://github.com/uppin/tddy-coder/pull/770)). The project-home agent
claims a session's terminal control lease and types into it. Across:

- `tddy-tool-engine` — new `terminal_keys.rs`: `encode_keys(text, keys, bracketed_paste)`, a pure
  table (16 + 1 unit tests, `tests/terminal_keys.rs`). `project_home_terminal_tools.rs` gains
  `CLAIM_SESSION_TERMINAL`, `SEND_SESSION_KEYS`, their defs, and two required methods on
  `ProjectHomeTerminalPort` (`claim_terminal`, `send_keys`). Both are target tools; registered in
  `is_project_home_tool`, `is_project_home_target_tool` and the read-only pin.
- `tddy-daemon-rpc` — new `exec_tool/home_terminal_leases.rs`: `HomeTerminalLeases`, keyed by
  `HomeTerminalClaimKey { caller_daemon_instance_id, caller_session_id, target_session_id }`, held on
  `ProjectHomeHost` (6 tests, `tests/home_terminal_leases.rs`). `DaemonProjectHome` implements the
  claim (`TerminalControl::claim`, `steal = true`) and the send (holder, remembered token, then an
  explicit `verify`, then `send_input(bytes, 0)` on `main`). `home_caller_confirmed_at_owner` returns
  the owning peer's instance id, which `project_home_port` now takes. 8 acceptance tests
  (`tests/project_home_terminal_keys_acceptance.rs`), including a jailed child, a cross-host claim and
  a second home that never claimed.
- `tddy-session-lifecycle` — `DaemonSessionHost::terminal_control()` is public; the terminal RPCs use it too.
- `tddy-tools` — both tools in the admitted list. `tddy-agent-launch` — the project-home prompt names them.

Decisions:

- The token is daemon-held, never returned, so the agent cannot replay it through another route.
- `TerminalControl::verify` accepts any token while nothing holds the lease, so a send first requires
  that the holder is this home's screen and a token is remembered. The first claim on an unheld
  session broadcasts no `ControlChange` (only a steal does).
- Paste mode is read through `read_terminal` only when there is text; a failed read refuses the send
  rather than falling back to raw typing.
- **Paste-end in text (settled at wrap).** Text holding `ESC[201~` under bracketed paste is
  **refused**, not stripped: stripping breaks the verbatim-paste rule and sending it ends the paste
  early. Raw typing is unaffected.
- **Jailed terminal size (checked at wrap).** A terminal with no known size refuses `text`. Production
  jails do report one: `tddy-sandbox-runner` pushes `TerminalResized` right after `openpty`, before the
  child can write, and after every resize (`runner.rs` `push_terminal_size`), and the host's
  `forward_jail_terminal_events` resizes the capture. Only the test's stand-in jail never reports a
  size, so no TODO is recorded.

Code issues: `home_caller_confirmed_at_owner` 69 → 49 lines by extracting
`ask_every_peer_for_sessions` (its record is closed). `exec_tool/project_home.rs` 686 → 779,
`project_home_tools.rs` 709 → 719 and `tddy-tool-engine/src/lib.rs` 885 → 886 production lines; the
splits are deferred because `#home-inbox` 6/8–8/8 touch those files, and each record carries the new
measurement.
