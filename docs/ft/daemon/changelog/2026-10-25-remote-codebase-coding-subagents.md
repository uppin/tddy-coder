# 2026-10-25 — Coding subagents run on the codebase host in remote-codebase mode

A remote-codebase session can carry coding subagents that run where the code is.
`tddy-tools remote start-session --agent <name@codebase-daemon>` (repeatable) seeds the `workspace`
session's roster with coding agents the codebase daemon offers; a model agent, or a coding agent of
another daemon, is refused naming it, and a refused start leaves no session behind. `tddy-tools --mcp`
reaches the roster stream and the conversation RPCs through the local relay over its HTTP endpoint, and
the codebase daemon runs each agent as the session's OS user on the workspace worktree — or, with
`clone` placement, on the conversation's own worktree there, with diff and pull carried back through the
relay. Every `tddy-tools remote` request and every daemon-HTTP call is protobuf; `tddy-tools remote`
prints canonical proto3 JSON (camelCase keys, defaults omitted), and a `ConversationWorktree` reply with
no `result_json` is an error.

Known gap: the relay ↔ codebase-daemon path is verified in one process, not across a real LiveKit room;
the two-daemon suite is not run.

See [Remote-codebase mode § Coding subagents on the codebase host](../remote-codebase-mode.md#coding-subagents-on-the-codebase-host).
