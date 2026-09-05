# 2026-08-30 — `--workspace-tools <worktree>` serves tool calls and spawns no agent

**Type:** Feature

the runner assumed it was hosting one: `--model` was mandatory and the normal path spawns a claude/cursor PTY. A workspace jail has neither, so the new mode answers an `in_jail_tool_request` by running `tddy_tool_engine::execute_tool` against the worktree as mounted inside the jail, which is what makes the boundary the kernel's rather than the tool engine's path checks. The flag lives on the `main.rs` `Cli` rather than in `SandboxRunnerArgs`, which three test files build as a struct literal; `--model` gains an empty default and the two agent paths now refuse an empty one explicitly, so "an agent needs a model" is still enforced where it means something. No tool-IPC server is started — that socket exists so an in-jail `tddy-tools --mcp` can call out, which a jail with no agent never does. (tddy-sandbox-runner)
