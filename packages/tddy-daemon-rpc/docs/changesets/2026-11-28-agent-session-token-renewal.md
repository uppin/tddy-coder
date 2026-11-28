# 2026-11-28 — The reserved agent-token-renewal exec-tool name

**Type:** Feature · **PR:** [#1064](https://github.com/uppin/tddy-coder/pull/1064)

`exec_tool/agent_token_renewal.rs`: `ExecToolRpcHandler` answers the reserved
`tddy.token.RenewAgentSession` call in `ports.rs` before anything routes it, so it is never
forwarded to a peer. A call that arrived over LiveKit, or names a `daemon_instance_id` other than
this daemon's, is `PERMISSION_DENIED`; a daemon that signs no session tokens answers
`FAILED_PRECONDITION`; otherwise `SessionTokens::renew_agent` re-mints the agent's token and the
answer carries the new token and its expiry, with the renewal (and every refusal) logged. New
dependency `tddy-session-tool-client` for the reserved name's constant — the client and the daemon
cannot disagree about it. See [architecture.md](../architecture.md).
