# 2026-11-03 — Every jail runs a tool plan: the sandbox runner and recipes forget coding agents

**Type:** Feature

`#agent-tools` 10/12 ([#799](https://github.com/uppin/tddy-coder/pull/799)). Packages:
`tddy-sandbox`, `tddy-sandbox-darwin`, `tddy-sandbox-cgroups`, `tddy-sandbox-runner`,
`tddy-sandbox-recipes`, `tddy-coding-agent-specs`, `tddy-sandbox-app`, `tddy-session-split`,
`tddy-daemon-sandbox`, plus test fixtures in `tddy-daemon-rpc`, `tddy-testing-commons` and
`tddy-tools`.

- `SandboxPlan.{context_dir: Option<PathBuf>, stdio: bool}` and the matching `SandboxBuilder`
  setters. Seatbelt's write deny and both backends' stdio transport read the plan
  (`profile::context_dir_of`, `stdio_mode_of`). Nothing sniffs `--context-dir` / `--stdio` out of
  the runner's argv.
- `tddy-sandbox-runner`: the agent modes and flags are deleted (`--agent-kind`, `--claude-binary`,
  `--agent-binary`, `--claude-arg`, `--agent-arg`, `--permission-mode`,
  `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`, `--mcp-log-level`;
  `spawn_claude_pty`, `spawn_cursor_pty`, `harness_pty.rs`). It runs `--exec-plan`, `--pty-command`
  or `--workspace-tools`, and otherwise refuses with `nothing to run`.
  `notify_when_the_process_exits` waits on a detached thread instead of `spawn_blocking`. Before
  that, a runner whose server returned first could not shut down while its process lived.
- `tddy-sandbox-recipes`: `SandboxRecipe::{ClaudeCli, CursorCli, OpenCodeCli, MuseCli}`,
  `detect_recipe_from_argv`, `claude_runner_env_overlay`, `prepare_persistent_claude_home` and the
  agent MCP/credential writers are deleted. The Claude tool lists move to
  `tddy_coding_agent_specs::claude_tools`.
- `tddy-coding-agent-specs`: `coding_agent.variants` (`AgentVariant`, `VariantError`,
  `HOST_AGENT_VARIANT`, `CodingAgentSpec::variant_args`) and `claude.yaml`'s `host_agent` variant.
- `tddy-sandbox-app --agent <spec id>` (`agent_spec::{choose_agent, agent_exec_plan_with}`) launches
  through the registry, `lower_tool_jail` and `--exec-plan`. `AgentKind`, `--agent-kind`,
  `--claude-binary`, `--cursor-binary`, `--claude-home-dir`, `--cursor-home-dir` and the per-agent
  home seeding are deleted. The `claude-sandbox` launcher passes its resolved binary as
  `TDDY_CLAUDE_BINARY`.
- `tddy-session-split`: the host agent renders from the embedded `claude.yaml`'s `host_agent`
  variant and the spec's slots.
- `tddy-daemon-sandbox`: `build_allow_read_paths` reads the toolchain and the runner binary only.

## Behaviour changes

- A split session's host agent is also given `--setting-sources user`, and `LS` is withdrawn.
- `tddy-sandbox-app --codebase-mode sandboxed`: its host agent also withdraws `LS`. The variant's
  natives come before a subagent's `replaces:`, which changes the order of the `--disallowedTools`
  flags.
- `tddy-sandbox-app`'s jail home is `<tddyhome>/homes/<key>`, from the spec's `home:` section. The
  default model is the spec's `models.default`.
- `tddy-sandbox-app` config keys `claude_binary`, `cursor_binary`, `claude_home_dir` and
  `cursor_home_dir` are gone. The config is `deny_unknown_fields`, so an old config naming one is
  refused at load. The binary comes from the spec's env override (`TDDY_CLAUDE_BINARY`,
  `TDDY_CURSOR_AGENT`).
- On Linux, `tddy-sandbox-app --agent` other than `claude` is refused (the daemon path starts the
  default agent only).
- The app cannot bind Cursor's `node_launcher` / `node_entry` run params.
- A split session renders the embedded spec, so an operator's `<tddyhome>/tools/claude.yaml` does
  not reach it.

## Decisions

- **Deletions approved by the developer**: `harness_pty.rs`, `build_sandboxed_cursor_runner_env`, the
  runner's subagent-replaced-tools helpers, and the claude-oauth / `OPENCODE_CONFIG` assertions in
  `route_files.rs` and `route_secrets.rs`.
- **`tddy_sandbox` still names `CLAUDE.md` / `AGENTS.md`** (`MANAGED_CODEBASE_PREAMBLE`,
  `PREAMBLE_FILES`): the source scan `tddy_sandbox_and_tddy_sandbox_recipes_name_no_coding_agent`
  is `#[ignore]`d, and threading the list through the context walk is deferred to the
  `#agent-tools` sweep (`docs/dev/todo/2026-11-03-tddy-sandbox-still-names-the-guidance-files-of-two-agents.md`).
- **Recorded debt** (`docs/dev/todo/`): `2026-11-03-cgroups-has-no-context-dir-carve-out`,
  `2026-11-03-runner-model-flag-is-vestigial`, `2026-11-03-plan-requests-host-home-is-unread`,
  `2026-11-03-split-sessions-render-the-embedded-agent-spec`. The opencode/muse app entry
  (`2026-10-22-sandbox-app-cannot-start-opencode-or-muse`) is narrowed to the route.
- **Restructuring deferred to the `#agent-tools` sweep node** (developer). Code issues re-measured:
  - better: `runner.rs` 2,854 → 2,240, `run_sandbox_runner_inner` 261 → 181,
    `sandbox_session.rs` 985 → 882, app `main.rs` 976 → 957, app `spawn.rs` 823 → 663;
  - worse: `builder.rs` 596 → 628, `workspace_tool_sandbox.rs` 713 → 715,
    `sandboxed_session.rs` 737 → 740;
  - new: `tddy-sandbox-cgroups` `oversized-file-lib` (746 → 756).
  - `claude_tools.rs` (moved from `tddy-sandbox-recipes/src/claude_cli.rs`) is 1,000 lines, but 189
    are production lines (to the first `#[cfg(test)]`), so it is under budget and no record was filed.

Docs: [Sandbox builder](../../ft/coder/sandbox-builder.md),
[Sandboxed codebase mode](../../ft/coder/sandboxed-codebase-mode.md),
[`tddy-sandbox` architecture](../../../packages/tddy-sandbox/docs/architecture.md),
[Coding agent specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md).
