# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

The connection-service forwarding ports (activity, OS-user resolution, agent-clone provisioning, session
files, split-agent spawn, paired-codebase teardown, turn-end reporting) take a `CommonRoom` instead of the
bare room slot; no behaviour change at the default setting. The remote managed worktree split-start
deadline is `spawn_worker_request_timeout + peer_forward_timeout`. No file grew.
