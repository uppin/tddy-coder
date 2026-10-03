# 2026-10-03 — `sync_conversation_worktree`

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

`sync_conversation_worktree(conversation)` and `conversation_sync_request` in `src/conversation.rs`;
every request builder takes an `Op` (`conversation_op_request`); the Connect-JSON HTTP body is built
by `connect_json_body`, with `{"sync": {}}` as the sync's key. Unit tests pin the sync request and
its HTTP body, and the reset's body.

`oversized-file-lib`: 1,110 → 1,111 (the re-export) — row recorded.
