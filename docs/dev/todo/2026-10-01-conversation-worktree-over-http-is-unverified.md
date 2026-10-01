# 2026-10-01 — The `ConversationWorktree` call over the HTTP/Connect JSON path is unverified

**Category:** Missing test
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`

`ask_daemon_over_http` (`packages/tddy-session-tool-client/src/conversation.rs`) sends the
`ConversationWorktreeRequest` as Connect JSON. The `oneof op` is encoded as `{"pull":{}}` /
`{"remove":{}}`. That encoding has not been checked against the daemon's HTTP endpoint, and no test
covers this transport.

**Why deferred:** the HTTP transport needs a running daemon HTTP surface; the other
transports are covered at the request seam, and no fixture in `tddy-session-tool-client` serves the
daemon's HTTP surface.

**What closing it takes:** a test posting the request to the daemon's Connect endpoint (or a unit
test against the generated serde shape of `ConversationWorktreeRequest`) asserting the `oneof`
encoding the endpoint accepts and that a `Pull` / `Remove` reaches the handler.
