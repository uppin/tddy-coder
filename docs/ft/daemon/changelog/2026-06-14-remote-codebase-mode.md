# 2026-06-14 — Remote-codebase mode

- **Remote daemon**: workspace sessions (`session_type:"workspace"`) with git worktree, no PTY; `ExecuteTool` (Read, Write, StrReplace, Delete, Grep, Glob, Shell, Await, SemanticSearch, ReadLints) + `ListExecTools` RPCs; `contain_path` security; background shell jobs + Await polling.
- **Relay daemon** (`--relay`): joins LiveKit common room; `forward_to_peer` + per-peer `RpcClient` cache routes `ExecuteTool`/`ListExecTools` to named remote peer; `IdleTimeoutTracker` triggers graceful shutdown after idle timeout; external oneshot shutdown channel in `run_server`.
- **`tddy-tools remote`**: `list-tools` via `ListExecTools` Connect POST; `start-session`, `connect-session`, `sync-context` subcommands; lazy relay daemon startup via `ensure_relay_daemon`.
- **`tddy-coder --remote`**: `--remote-daemon-url`/`--remote-session-token`/`--remote-daemon-id` flags; `run_remote` shells out to `tddy-tools remote list-tools`, builds dynamic `mcp__tddy-tools__*` allowlist, runs free-prompting workflow with remote ctx keys and read-only local ctx dir.
- Feature: [remote-codebase-mode.md](../remote-codebase-mode.md). Cross-package: [docs/dev/changesets/](../../../dev/changesets/).
