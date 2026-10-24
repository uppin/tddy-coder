# 2026-10-24 — Split start streams, no deadline

**Type:** Feature

`#start-progress` 4/8 ([#737](https://github.com/uppin/tddy-coder/pull/737)). `split_forward_stream::forward_split_start` relays the codebase host's start events; `SplitStartFailure::{PeerAnswered, PeerSilent}` replaces `ForwardDeadline`; `STEP_AGENT` around the split agent spawn; `start_workspace_session` / `start_agent_clone_session` lose `request_timeout`. `start_split_claude_cli_session` grew 149 → 153 (code issue updated, deferred with consent). See [module layout](../module-layout.md). Cross-package entry: [docs/dev/changesets/2026-10-24-start-progress-split-stream.md](../../../../docs/dev/changesets/2026-10-24-start-progress-split-stream.md).
