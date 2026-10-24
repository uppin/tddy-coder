# 2026-10-24 — Agent terminal tabs: the daemon roster runs CLI harnesses
**Type:** Feature

`CliManagerTerminalRoster` holds the `DaemonConfig` and resolves `TerminalKind::Agent(kind)` through `cli_harness(kind)` (`INVALID_ARGUMENT` when unregistered) and `DaemonConfig::cli_harness_binary`, builds the descriptor's argv for a default `CliLaunch` with a fresh conversation id, and spawns it with `CliSessionManager::start_agent_terminal`. Pinned by `tests/agent_terminal_tabs_acceptance.rs` (real PTY, stub harness binaries). Deferred: `docs/dev/todo/2026-10-24-claude-agent-tab-shares-session-hooks.md` (#718).
