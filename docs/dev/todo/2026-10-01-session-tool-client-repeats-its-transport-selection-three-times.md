# 2026-10-01 — `tddy-session-tool-client` selects its transport in three copies

**Category:** Future enhancement — duplication
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`

The match that chooses between the sandbox channel, the daemon UDS, the daemon HTTP endpoint and
LiveKit exists three times: `dispatch_session_tool` (`src/lib.rs`), `dispatch_conversation_tool` and
`conversation_worktree` (`src/conversation.rs`).

**Why deferred:** the fix touches `src/lib.rs`, which #561–#563 also edit, and the file is over
budget (`packages/tddy-session-tool-client/docs/code-issues/oversized-file-lib.md`).

**What closing it takes:** a `with_transport` helper that resolves the transport once and hands it
to a closure per call kind; the three callers shrink to one line each, and `lib.rs` loses the
`dispatch_request_via_*` duplication this change added (+60 lines). After #563 lands.
