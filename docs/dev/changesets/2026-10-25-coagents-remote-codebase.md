# 2026-10-25 — Coding subagents on the codebase host, in remote-codebase mode

**Type:** Feature

`#coagents` 6/8 ([#725](https://github.com/uppin/tddy-coder/pull/725)). A `workspace` session can carry
coding subagents that run on the codebase daemon, reached from `tddy-tools --mcp` through the local
relay over its HTTP endpoint. Feature doc:
[remote-codebase-mode.md § Coding subagents on the codebase host](../../ft/daemon/remote-codebase-mode.md#coding-subagents-on-the-codebase-host).

## What landed

- **`tddy-tools`**: `remote start-session --agent <name@daemon>` (repeatable) → `specialized_agents`.
  Every `remote` subcommand speaks protobuf (`application/proto`) and prints canonical proto3 JSON
  through the `ConnectJson` builders in `remote_cli.rs`.
- **`tddy-session-tool-client`**: `daemon_http_rpc::DaemonHttpRpcClient`, a Connect-protocol
  `RpcClientTransport` over the daemon's `/rpc` (unary `application/proto`, server streams
  `application/connect+proto`). `ExecuteTool`, `ListExecTools` and `ConversationWorktree` over HTTP go
  through it (they were Connect JSON, which the daemon's `/rpc` mount does not decode — a defect on
  master). An empty `result_json` on any conversation transport is an error.
- **`tddy-discovery`**: the roster stream and `AgentConversationLink` accept `DaemonHttp`;
  `TODO(session-agent-roster)` removed with the refusal.
- **`tddy-session-agents`**: `workspace_roster::workspace_roster_admits` — coding agents of the serving
  daemon only.
- **`tddy-agent-launch`**: `LaunchSessions::admitted_workspace_seed` resolves and admits the workspace
  seed before anything of the session is written, when the start carries no `split_agent`; a clone
  start seeds nothing.
- **Dev-dependencies (developer-approved):** `axum = "0.8"` and `tddy-connectrpc` in
  `tddy-session-tool-client` and `tddy-daemon-rpc`, to serve the daemon's real `/rpc` router in tests.

## Decisions

- **No relay forward added.** The relay is an ordinary daemon; its peer routing already forwards by
  `daemon_instance_id`. What was missing was an HTTP client for it.
- **Remote placement is a per-conversation clone opened locally on the codebase daemon** (developer
  decision 2026-10-25): `clone` on a remote-codebase session is the conversation worktree that daemon
  serves through `open_local`. The admission rule is unchanged.
- **Split starts are exempt from workspace admission** — their roster lives on the agent daemon.
- **Accepted behaviour changes:** `tddy-tools remote` output is canonical proto3 JSON (camelCase, as
  before; proto3 defaults omitted — not compared byte for byte with the old output). A missing
  `result_json` is an error (`optional string result_json` in the proto was the alternative, not taken).
  Error wording on the HTTP arms of `ListExecTools` and `ConversationWorktree` follows their RPC arms.

## Verification

Scoped, local (2026-10-25, after rebasing onto `5a2c1ecf2`):

- `./test -p tddy-session-tool-client -p tddy-tools -p tddy-tools-cli -p tddy-session-agents -p tddy-discovery -p tddy-agent-launch` — 1,106 passed, 0 failed, 1 ignored.
- `cargo test -p tddy-daemon-rpc --test remote_codebase_clone_placement_acceptance --test roster_stream_over_daemon_http_integration` — 4 passed.
- `cargo test -p tddy-session-lifecycle --test workspace_seed_admission_integration` — 3 passed.
- `cargo clippy -p <those 8 packages> --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

**Known gap — NOT RUN:** the two-daemon LiveKit suite (`LIVEKIT_TESTKIT_WS_URL` unset; nothing faked).
The relay ↔ codebase path is verified in one process with an in-process forwarder for the hop. Backlog:
*Coding subagents in remote-codebase mode: the two-daemon LiveKit suite is not run* (2026-10-25).

## Backlog and code issues

- *The `ConversationWorktree` call over the HTTP path is unverified* (2026-10-01) — **narrowed**: `Diff`,
  `Pull` (through a relay's HTTP endpoint, tests 11–12) and `Reset` reach the handler as protobuf;
  `Remove`, `PullRange` and `Sync` remain.
- Added: *Coding subagents in remote-codebase mode: the two-daemon LiveKit suite is not run* (2026-10-25).
- Code issues recorded, not solved (developer decision: the `#coagents` stack defers code issues to the
  sweep): `packages/tddy-tools/docs/code-issues/oversized-file-remote-cli.md` (357 → 509, this PR
  crossed 500); `packages/tddy-discovery/docs/code-issues/oversized-file-roster-stream.md` (551 → 545,
  already over); `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`
  (384 → 392, regressed by the seed admission branch).
