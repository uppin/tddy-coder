# 2026-10-25 — TerminateSession stops a session as the UI's Terminate does

**Type:** Feature

`#start-parity` 7/9 ([#756](https://github.com/uppin/tddy-coder/pull/756)). A project-home session
can stop a session of its project on any eligible host with `TerminateSession {session_id,
daemon_instance_id?}`: SIGTERM to the session's process, as the web UI's Terminate sends it through
`SignalSession`. The record, worktree and branch are kept, so the session can be resumed; a session
whose process has ended is refused saying so, and another project's session is refused and left
running. Across-host calls ride `#start-parity` 1/9's whole-call forward unchanged. Decision:
**parity means `SignalSession` SIGTERM, not `DeleteSession`**; SIGINT, SIGKILL and Delete are not
offered. The `SignalSession` handler's body became `signal_session_dir(session_dir, session_id,
signal)` so the tool signals the session directory the port resolved the target from, instead of
re-deriving it from the OS user. Across:

- `tddy-tool-engine` — `project_home_terminate.rs`: the definition and handler; `ProjectHomePort::terminate_session`, catalogue entry and dispatch in `project_home_tools`.
- `tddy-agent-launch` — `svc_signal_delete_session::signal_session_dir`, shared by `SignalSession`.
- `tddy-daemon-rpc` — `DaemonProjectHome::terminate_session`; new dependency on `tddy-agent-launch`.
- `tddy-tools` — the project-home allowlist test names the tool.

Code issues (split deferred, stack-shared): `tddy-daemon-rpc/src/exec_tool/project_home.rs`
730 → 746, `tddy-tool-engine/src/project_home_tools.rs` 632 → 643, `tddy-tool-engine/src/lib.rs`
882 → 883. No TODO added.
