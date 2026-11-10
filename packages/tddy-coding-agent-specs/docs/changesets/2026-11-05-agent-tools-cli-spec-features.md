# 2026-11-05 — managed_workflow and the conversation slot

**Type:** Feature

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)).

- `Capability::ManagedWorkflow`, declared by `claude.yaml` and `cursor.yaml`.
- `slots::conversation::ConversationSlot` at `coding_agent.conversation` (`daemon_session_id` for claude, `minted` by `cursor-create-chat` for cursor; derived `resume_token` / `worktree` for the rest), `CodingAgentSpec::conversation()`, and its load refusals.

See [coding-agent-specs.md](../coding-agent-specs.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-cli-spec-features.md).
