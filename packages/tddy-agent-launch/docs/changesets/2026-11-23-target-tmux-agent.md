# 2026-11-23 — A target session's agent runs in tmux on the target

**Type:** Feature

`RemoteLaunch::tmux`, `remote_agent_tmux_command`, `end_target_tmux_session`, `probe_tmux_on_target`, `RemoteAgentPid` (start wait and pointer owner read the tmux pane's pid), and the delete / resume / startup paths that never act on the host's tmux for a target session. Full record: [`docs/dev/changesets/2026-11-23-target-tmux-agent.md`](../../../../docs/dev/changesets/2026-11-23-target-tmux-agent.md) (#1019).
