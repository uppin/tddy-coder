# 2026-10-24 — Routing the answer, naming the answerer

**Type:** Feature

`#coagents` 8/8 ([#727](https://github.com/uppin/tddy-coder/pull/727)). `PeerRoutedSessionAgents::answer_agent_permission` routes on `daemon_instance_id` before session lookup. `TurnLoopsThisDaemonRuns`: `github_user` (refuses an agent-principal token `PERMISSION_DENIED`) and `answerer` (`MainAgent` only for an agent token this daemon's keys verify). The answer forward to the owning daemon. Test support: `test_util::signing_test_host` with `SignedTestCredentials { user_token, agent_token }` (`test_util.rs` 367 → 448 lines).
