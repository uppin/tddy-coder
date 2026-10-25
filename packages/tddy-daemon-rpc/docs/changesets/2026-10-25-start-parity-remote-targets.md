# 2026-10-25 — Project-home tools across hosts

**Type:** Feature

`#start-parity` 1/9 ([#750](https://github.com/uppin/tddy-coder/pull/750)). New
`exec_tool/project_home_remote.rs`: `served_on_target_host` (forward a target tool call to the host its
`daemon_instance_id` names), `PeerSessionLists` (the `ListProjectSessions` fan-out, unreachable hosts
named) and `serve_project_home_for_caller_held_elsewhere` (a caller this daemon does not hold is
confirmed by the owning daemon's `ListSessions` row — `session_role == project-home` — and served for this
host only, so a call travels one hop). Wired into both `ExecuteTool` and `StreamExecuteTool`.
`DaemonProjectHome` gains `daemon_instance_id` and `peers`; `project_home_port` and `execute` factored
out of `run_project_home_tool`. No proto change.

Tests: `tests/project_home_remote_targets_acceptance.rs` — two in-process daemons joined by an
in-memory peer forwarder (no LiveKit). It carries its own two-host world; `#start-parity` 2/9 moves it
into `tests/support/mod.rs`.
