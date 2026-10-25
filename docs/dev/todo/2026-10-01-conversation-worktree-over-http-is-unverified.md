# 2026-10-01 — The `ConversationWorktree` call over the HTTP path: `Remove`, `PullRange` and `Sync` are unverified

**Category:** Missing test
**Source:** `#agent-worktree` 1/4, changeset `2026-09-30-agent-worktree-isolated-edits`
**Status:** Open — partially fixed (`#coagents` 6/8, #725: `Diff`, `Pull` and `Reset` verified; `Remove`, `PullRange` and `Sync` remain)

`ask_daemon_over_http` (`packages/tddy-session-tool-client/src/conversation.rs`) sends the
`ConversationWorktreeRequest` to the daemon's `/rpc` mount through `DaemonHttpRpcClient`, as protobuf
(`application/proto`), the encoding the generated handler decodes.

## Measurement history

| Date | Change | What reaches the handler over HTTP |
|---|---|---|
| 2026-10-01 | `#agent-worktree` 1/4 | nothing tested; the request was Connect JSON (`{"pull":{}}` / `{"remove":{}}`) |
| 2026-10-25 | `#coagents` 6/8 (#725) | the Connect-JSON body is gone (protobuf). `Diff` and `Pull` through a relay's HTTP endpoint reach the codebase host's handler and answer from its worktree (`packages/tddy-daemon-rpc/tests/remote_codebase_clone_placement_acceptance.rs`, tests 11–12: relay and host in one process, each on the daemon's real `/rpc` mount). `Reset` reaches the generated `ConversationWorktree` handler decoded, naming its commit (`packages/tddy-session-tool-client/tests/conversation_reply_protobuf_acceptance.rs`). `Remove`, `PullRange` and `Sync` are not sent over HTTP by any test |

## What would close it

A test sending `Remove`, `PullRange` and `Sync` over the HTTP arm to the daemon's `/rpc` mount,
asserting each reaches the `ConversationWorktree` handler as that operation. All three share the
encode path the verified operations take (prost `oneof op`), so this is a test, not a code change.
