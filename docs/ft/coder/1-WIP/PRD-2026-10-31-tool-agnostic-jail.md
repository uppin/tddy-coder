# Every jail runs a tool plan: the sandbox runner and recipes forget coding agents - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement
**Stack**: `#agent-tools 10/12` (branch `feature/agent-tools/jail-agnostic`)

## Affected Features

- **Primary**: [Sandbox builder](../sandbox-builder.md). The runner and recipes carry no agent vocabulary.
- **Related**: [Sandboxed codebase mode](../sandboxed-codebase-mode.md), the standalone `tddy-sandbox-app`, and split sessions ([PRD-2026-08-31-split-sandbox-orchestration](../../daemon/amendments/PRD-2026-08-31-split-sandbox-orchestration.md)). All of them launch from specs.

## Summary

The last consumers of the agent-aware jail move onto tool plans:

- **`tddy-sandbox-app`** takes `--agent <spec id>`, which replaces `--agent-kind` and its `AgentKind` enum. It drops its duplicate Claude-home seeding.
- **Split sessions**, both the Claude host agent and the workspace jail, launch from `claude.yaml`, using `--strict-mcp-config --setting-sources user` as a declared host-agent variant.

After that, the agent code is deleted:

- **From the runner**: `--agent-kind`, `--claude-binary`, `--agent-binary`, `--claude-arg`, `--agent-arg`, `--permission-mode`, `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`, `spawn_claude_pty`, `spawn_cursor_pty` and the in-runner MCP writers.
- **From the recipes**: the `ClaudeCli`, `CursorCli`, `OpenCodeCli` and `MuseCli` arms, `detect_recipe_from_argv`, `claude_runner_env_overlay`, and argv sniffing in the daemon's `build_allow_read_paths` and in the darwin profile's `--context-dir` and `--stdio` detection. The plan says these things explicitly instead.
- **From the generic crate**: `MANAGED_CODEBASE_PREAMBLE` and `PREAMBLE_FILES` move out of `tddy-sandbox`, into the `instructions` slot renderer. `allow_oauth_inbound` becomes a plain `NetworkSpec` field, set only from tool declarations.

## Acceptance Criteria

- [ ] `tddy-sandbox-app --agent claude` and `--agent cursor` start jails from specs, with the same behaviour as today.
  - `--agent opencode` is refused while it has no route, as today ([2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md](../../../dev/todo/2026-10-22-sandbox-app-cannot-start-opencode-or-muse.md)).
- [ ] A split session's host agent and workspace jail start from specs, and the existing split acceptance suite passes unchanged.
- [ ] `tddy-sandbox-runner --help` lists no agent flag. Passing `--agent-kind` is an error.
- [ ] `tddy-sandbox-recipes` exports no agent-named item, and `tddy-sandbox` contains no `claude` or `cursor` identifier (pinned by a source scan test).
- [ ] The darwin profile and cgroups backend read `--stdio` and the context dir from the plan, not from argv.

## Out of Scope

- Giving the standalone app a provider route.
- The `warp-cli` jail, unless `8/12` took it.
