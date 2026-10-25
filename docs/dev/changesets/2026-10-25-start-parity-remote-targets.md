# 2026-10-25 — List and reach project sessions on every host

**Type:** Feature

`#start-parity` 1/9 ([#750](https://github.com/uppin/tddy-coder/pull/750)), root of the stack. A
project-home session's `ListProjectSessions` fans out across every eligible host, and its target tools
(read, diff, git) forward a call naming another host to that host, which confirms the caller with the
daemon that owns it (no proto change — a delegated-caller field would have touched ~60
`ExecuteToolRequest` literals, and an asserted caller cannot be trusted over a transport that also
carries browsers). Across:

- `tddy-tool-engine` — the target-host argument and the cross-host listing types.
- `tddy-daemon-rpc` — `exec_tool/project_home_remote.rs`: the forward, the fan-out and the confirmation
  at the owner.

Code issues: `tddy-tool-engine/src/project_home_tools.rs` 425 → 500 production lines, split deferred
(stack-shared). Pending: the acceptance test's two-host world moves into `tests/support/mod.rs` in
`#start-parity` 2/9.
