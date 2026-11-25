# 2026-11-25 — Follow an agent's transcript on host and target; hook-log follower resumes from the last delivered byte

**Type:** Feature + Fix

`log_follower::FileFollower` (`stty -opost`, delivered-byte offsets, persisted `FollowOffset`, INFO start/break/reopen logs) behind the hook log and `transcript_follower`; `create_remote_hook_log` keeps an existing log; `RemoteHookFollowers` holds both followers and aborts a replaced one; `harness_followers` starts them and keeps them across a tmux client's exit while the agent's tmux session lives. Full record: [`docs/dev/changesets/2026-11-25-transcript-activity-source.md`](../../../../docs/dev/changesets/2026-11-25-transcript-activity-source.md) (#1035).
