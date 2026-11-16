# 2026-11-16 — A remote agent's hooks reach the daemon with nothing installed

**Type:** Feature · `#remote-shell` 6/7, PR [#920](https://github.com/uppin/tddy-coder/pull/920)

`remote_hook_follower.rs` follows a target session's hook log through its remote shell:
`HookLine::parse` checks the token first and drops (counted) a wrong token, an unknown event or
a broken payload; `RemoteHookFollower` reads with `tail -c +<offset+1> -f` and resumes from the
byte it reached, so no hook is lost or delivered twice; `ActivityHookSink` maps each line
through `tddy_session_activity::hook_request` and serves it in-process; the launch creates the
log before the agent starts, and exit, stop and delete remove it. The startup window in
`remote_agent_launch` runs from the agent's recorded pid, and an agent that never records one
is refused as `AgentNeverStarted`. `remote_agent_launch.rs` is 546 production lines; its split
is recorded in `docs/code-issues/oversized-file-remote-agent-launch.md` and deferred until no
stack PR still edits it. Full record:
[`docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-remote-hooks.md).
