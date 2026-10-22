# 2026-10-22 — CLI harness sessions listed from the registry

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `session_list_status_from_session_dir` answers every registered CLI harness session type from
`.session.yaml` (agent = the session type); `worktree_removal_applies_to("opencode-cli")` is true, so
deleting an opencode-cli session removes its daemon-managed worktree. See
[OpenCode CLI session](../../../../docs/ft/daemon/opencode-cli-session.md#resume-and-delete).
