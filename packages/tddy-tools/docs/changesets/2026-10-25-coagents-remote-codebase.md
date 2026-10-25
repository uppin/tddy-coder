# 2026-10-25 — `remote start-session --agent`; the `remote` wire is protobuf

**Type:** Feature

`tddy-tools remote start-session --agent <name@daemon>` (repeatable) sends each agent as a
`specialized_agents` entry of the workspace `StartSession`. `connect_post` is generic over prost request
and response types and posts `application/proto`; each subcommand prints its decoded answer as canonical
proto3 JSON through the `ConnectJson` builders. `remote_cli.rs` is 509 production lines, recorded in
`docs/code-issues/oversized-file-remote-cli.md`. `#coagents` 6/8, [#725](https://github.com/uppin/tddy-coder/pull/725);
see [remote-codebase mode](../../../../docs/ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
