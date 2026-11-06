# 2026-11-04 — ACP coding agents launch from their tool definitions

**Type:** Feature

`#agent-tools` 11/12 ([#800](https://github.com/uppin/tddy-coder/pull/800)). Packages:
`tddy-actions`, `tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-coding-subagent`,
`tddy-session-agents`, `tddy-session-lifecycle`, `tddy-coder` (plus a one-field fixture in
`tddy-build` and a widened registry-id list in `tddy-sandbox-app`'s test).

- `tddy-actions`: `ToolBinary.sibling_of` and `tool::resolve_sibling` — a binary in the directory of a
  dep's resolved binary, refused naming both when absent; `PATH` is not searched.
- `tddy-coding-agent-specs`: `specs/claude-acp.yaml`, `specs/codex-acp.yaml` (`sibling_of: codex`,
  `hooks_adapter: codex-acp-login`, `tool_deny: unsupported`), and `opencode.yaml`'s
  `coding_agent.acp` (`opencode acp`, `opencode-route-file`, `tool_deny: { config_env:
  OPENCODE_CONFIG_CONTENT }`). `AcpSection`, `run.transport: acp`, `McpSlot::AcpSession`,
  `ToolSlot::{AcpMeta, ConfigEnv}`, `slots::acp::{acp_mcp_servers, acp_withdrawal}`,
  `package_runner`.
- `tddy-agent-backend`: the ACP hooks registry `AcpHooksAdapters` (`AcpHooksAdapter`,
  `AcpOpenScope`, `PreparedAcpHooks`) with `codex-acp-login` and `opencode-route-file` (today's
  `CodexLoginHooks` and `RouteFiles`, moved). `ClaudeAcpBackend::with_command`,
  `CodexAcpBackend::with_launch`, `OpenCodeAcpBackend::with_launch`.
- `tddy-coding-subagent`: `confinement_of_spec`, `refuse_unless_spec_confinable`, and
  `CodingAgentLauncher::session_meta_for`, which `tddy-session-agents`' `start_acp_session` now asks.
- `tddy-session-lifecycle`: `DaemonCodingAgentLauncher` plans `claude-acp`, `codex-acp` and
  `opencode` from their specs when it holds coding agents (`acp_plan_from_spec`,
  `acp_mcp_servers_from_spec`, `acp_session_meta_from_spec`, the `session_meta_for` override).
- `tddy-coder`: `acp_from_spec::{claude_acp_backend_from_spec, codex_acp_backend_from_spec,
  opencode_acp_backend_from_spec}` behind `--agent claude-acp|codex-acp|opencode`.
- **Deleted** (approved 2026-11-04): `codex_binaries::resolve_codex_acp_binary`,
  `codex_acp_beside_resolved_codex`, `resolve_executable_on_path`.

## Decisions

- **Behaviour change: the daemon's ACP launches run from their specs as soon as #800 lands**,
  through `for_daemon` → `host_coding_agents`, with no config gate. The changeset's premise that no
  production path installed the coding agents was stale (developer, 2026-11-04).
- **`codex-acp` resolves beside the resolved `codex`, never from `PATH`** — in the daemon and in
  `tddy-coder --agent codex-acp`. `TDDY_CODEX_ACP_CLI` / `--codex-acp-cli-path` still name it.
- `claude-acp.yaml`'s `bunx` vs a bare `TDDY_CLAUDE_AGENT_ACP_CLI` binary is the `package_runner`
  template param (kept, developer).
- `choose_agent` keeps offering the ACP agents to the PTY sandbox
  ([todo](../todo/2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md)); `tddy-sandbox-app`'s
  unknown-id test lists `claude-acp` and `codex-acp`.
- Deny slots are `ToolSlot` variants written `{ acp_meta: <path> }` / `{ config_env: <VAR> }` (the
  type is untagged); the MCP kind is `acp-session`; `AcpSection` carries its own `mcp`, `tool_deny`
  and `withdraw_native`. The `-m` → `-c model=` rewrite stays `codex_acp_route_args` until #801.

## Deferred (to the `#agent-tools` sweep node, developer 2026-11-04)

- **The hand-built ACP path stays live** for launchers built without coding agents (`new`,
  `with_identity`; tests only): `harness_command`, `apply_route`, `claude_agent_acp_command`,
  `opencode_route_config`, the ACP constants, `opencode_withdrawal_config`, `ACP_HARNESSES`,
  `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`, `withdrawal_session_meta`. The plan deleted them;
  the sweep migrates the fixtures first
  ([todo](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md)).
- **OpenCode's route config stays inline** (`OPENCODE_CONFIG_CONTENT`) in the daemon launcher: no
  session-scoped owner for the route file
  ([todo](../todo/2026-11-04-opencode-acp-route-file-has-no-session-owner.md)).
- **Unbounded waits in parent code** the ACP launch reaches: `AcpDriver::open_session` and the
  supervisor client's `call_unary` (via `coding_agent_relay` `spawn_relayed`)
  ([todo](../todo/2026-11-04-acp-launch-waits-have-no-deadline.md)).
- `TODO(acp-agents)` markers left with their reason: `ClaudeAcpBackend::default` (until
  `model_catalog` reads the spec), `CodingAgentExtension::validate` (cannot see `run.transport`, so an
  `acp` transport with no `acp` section is refused at launch, not load), the
  `session_meta_for` default and the hand-built launcher branch (the sweep above).
- **Restructuring deferred**: new `oversized-file-spec` (`tddy-coding-agent-specs`, 492 → 656);
  regressed `oversized-file-coding-agent-launcher` (1036 → 1291) and `oversized-file-run`
  (2637 → 2646).

Product docs: [changelog](../../ft/coder/changelog/2026-11-04-acp-agents-from-specs.md),
[Codex ACP backend](../../ft/coder/codex-acp-backend.md),
[OpenCode ACP backend](../../ft/coder/opencode-acp-backend.md),
[Session agent roster](../../ft/daemon/session-agent-roster.md).
