# 2026-10-25 — StartSession returns at once; poll or cancel the start

**Type:** Feature

`#start-parity` 4/9 ([#753](https://github.com/uppin/tddy-coder/pull/753)). The orchestrator's
`StartSession` tool opens the streamed start and returns `{start_id, task_id, daemon_instance_id}` as
soon as the start names its task. A consumer on the caller's host drains the stream into a start
ledger persisted under the home session's `artifacts/session-starts/`, read by `GetSessionStart`;
`CancelSessionStart` cancels the task locally or on the peer running it. A record a restart left
`running` reads `failed`. Developer decision: **a cancelled start deletes the branch it cut** (local
only), for every cancel of a streamed start, the web UI's included. Across:

- `tddy-tool-engine` — `start_ledger.rs` and the two new tools.
- `tddy-daemon-rpc` — the port methods, local and forwarded cancel, the two-host test world's
  `StreamStartSession` / `CancelTask` routes.
- `tddy-agent-launch` — `start_cut_branch.rs`, the cancel undo's branch deletion.

Code issues (split deferred, stack-shared): `tddy-tool-engine/src/project_home_tools.rs` 457 → 558,
`tddy-tool-engine/src/lib.rs` 879 → 880, `tddy-daemon-rpc/src/exec_tool/project_home.rs` 478 → 665
(new record). TODOs added: a pushed branch is not deleted from origin on cancel; a forwarded start's
caller-side cancel can report "left in place".
