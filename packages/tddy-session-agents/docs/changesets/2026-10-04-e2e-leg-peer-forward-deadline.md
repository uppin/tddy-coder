# 2026-10-04 — The peer-forward deadline is a daemon setting

**Type:** Feature

`#e2e-leg` 2/5 ([#579](https://github.com/uppin/tddy-coder/pull/579)); cross-package entry:
[2026-10-04-e2e-leg-peer-forward-deadline.md](../../../../docs/dev/changesets/2026-10-04-e2e-leg-peer-forward-deadline.md).

The conversation open and cancel forwards and the hosted-clone start take a `CommonRoom`.
`SessionAgentCloneSpec.common_room_slot` is renamed `common_room` and holds one. `session_agent_clone.rs`
production lines 1,158 -> 1,157 (`oversized-file-session-agent-clone` stays open, row added).
