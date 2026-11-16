# 2026-11-16 — A remote agent's hooks reach the daemon with nothing installed

**Type:** Feature · `#remote-shell` 6/7, PR [#920](https://github.com/uppin/tddy-coder/pull/920)

`hook_request.rs` is the one payload-to-request mapping of an agent's lifecycle hook:
`session_status_request` and `agent_activity_request`, moved from `tddy-tools` (which calls it
too), so the local `tddy-tools session-hook` and a target session's hook-log follower in
`tddy-agent-launch` map a payload the same way. Full record:
[`docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md).
