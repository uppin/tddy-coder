# 2026-11-05 — The cursor-create-chat conversation-id adapter

**Type:** Feature

`#agent-tools` 14/19 ([#820](https://github.com/uppin/tddy-coder/pull/820)).

- `adapters::conversation_id::{ConversationIdAdapter, ConversationMint, ConversationMintError, CursorCreateChat}`, registered in `LaunchAdapters::builtin()` (`conversation_id`, `conversation_id_names`): runs `<binary> create-chat` in the workdir, returns the bare chat id it prints, and is killed at its deadline (`ConversationMintError::TimedOut`). `parse_created_chat_id` moved here.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-05-agent-tools-cli-spec-features.md).
