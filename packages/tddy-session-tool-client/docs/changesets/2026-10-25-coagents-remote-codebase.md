# 2026-10-25 — A Connect client for the daemon's HTTP `/rpc`

**Type:** Feature

`daemon_http_rpc::DaemonHttpRpcClient` (public): unary calls as `application/proto`, server streams as
enveloped `application/connect+proto`; Connect error objects map to their code, anything else is
`Unknown`/`Unavailable`/`Internal` as the README states. `ExecuteTool`, `ListExecTools` and
`ConversationWorktree` over `DaemonHttp` go through it as protobuf (`connect_json_body` removed). A
conversation reply with an empty `result_json` is an error on every transport. Dev-dependencies:
`axum 0.8`, `tddy-connectrpc`. `#coagents` 6/8, [#725](https://github.com/uppin/tddy-coder/pull/725);
see [remote-codebase mode](../../../../docs/ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
