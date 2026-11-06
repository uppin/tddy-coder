# Coding agent specs (`tddy-coding-agent-specs`)

Product contract: [Claude CLI session § Launch](../../../docs/ft/daemon/claude-cli-session.md#launch-from-the-claude-tool-definition).
Tool schema: [`tddy-actions` declared tools](../../tddy-actions/docs/declared-tools.md).

A coding agent is a `tddy_actions::tool::ToolDef`:

- its `run:` section is the argv template;
- its `deps` name the tools it calls back through (`tddy-tools`);
- its `extensions.coding_agent` section declares the tddy **mapping slots**.

This crate owns that section's vocabulary. `tddy-actions` stays agent-agnostic.

| Module | Holds |
|---|---|
| `lib.rs` | `builtin_specs()` (the embedded `specs/*.yaml`). `load_agent_registry(overrides_dir)`: `BUILTIN_TOOLS ++ BUILTIN_SUPPORT_TOOLS ++ builtin_specs()` through `ToolRegistry::from_embedded`, then `load_with_owners` with `<tddyhome>/tools` and `[CodingAgentExtension]` |
| `spec.rs` | `CodingAgentExtension`, the `ExtensionOwner` for `coding_agent`, which deserializes the section. `CodingAgentSpec::of(&ToolDef)` returns `{ id, run, agent: CodingAgentSection }`, or `SpecError::{NotACodingAgent, NoRunSection, Invalid}`. The section types are `McpSlot::File { flag, format: McpFormat::ClaudeMcpJson, strict_flag }` and `McpSlot::Placed { path, format: McpFormat::CursorMcpJson }` (a file the agent reads from a fixed place, `path` may name `{home}`), `ToolSlot = Flag(FlagSlot) \| unsupported` for `tool_allow` / `tool_deny` / `permission_prompt`, `InstructionsSlot { files, append_system_prompt, rules_file }`, `HooksSlot { adapter }`, `ModelsSection { default, curated }` and `context_globs` |
| `spec.rs` (headless) | `HeadlessSection { argv, env, adapter, exit_ok_if_structured, mcp: Option<McpSlot> }` at `coding_agent.headless` (`CodingAgentSpec::headless()`, `run_section()` as a `stdio` run section). `CodingAgentExtension::validate` refuses an `adapter` that `tddy_agent_backend::adapters::AdapterRegistry::builtin()` does not hold |
| `headless.rs` | `validate_headless` (`HeadlessSpecError::{NoHeadlessSection, UnknownAdapter}`, naming the adapter and the registered ones). `SpecHeadlessRenderer::new(spec, tool, deps, adapters)` implements the backend's `HeadlessRenderer` port: `slots::render` (no preamble, no hooks; `headless.mcp` overrides `coding_agent.mcp`), then `render_argv` with `prompt`, `model`, `permission_mode`, `workdir`, `session_id` / `resume`, each turn `param`, and the lists `slots` and `route_args`. `tool_server()` is the resolved `tddy-tools` dep (`TOOL_SERVER_DEP`) run with `--mcp` |
| `slots/mod.rs` | `SlotInputs` (workdir, scratch dir, MCP server, allowed / denied tools, permission-prompt tool, preamble, append-system-prompt file, hook params, and `home` — what `{home}` resolves to: the jail's persistent home or the host's `$HOME`). `render(&CodingAgentSpec, &SlotInputs) -> RenderedSlots { argv_extra, files }`. `SlotError::{UnknownHooksAdapter, Io, SlotUnsupported, NoDaemonSocket, UnboundHome}` |
| `slots/mcp.rs` | a `File` slot: the registration file `<scratch>/claude-mcp-config.json` (`{"mcpServers": {<name>: {command, args, env?}}}`), and `flag <path> [strict_flag]`. A `Placed` slot: the registration at its resolved path, and no argv |
| `slots/headless_mcp.rs` | the headless MCP kinds: `Inline { flag, key_prefix }` renders `-c mcp_servers.<n>.{command,args,env_vars}=…` (TOML values; env vars named, never valued), `Argv { flag }` writes a servers-by-name file (`ARGV_MCP_FILE`) owner-only under the scratch dir and renders `--mcp <path>`, `Workdir { path, format }` writes `<workdir>/<path>` (`cursor-mcp-json`). `mcp_written_paths` lists every kind's files, which `RenderedSlots.written` carries so the launcher records them in `headless/<conversation>/written-files` |
| `slots/tools.rs` | one flag per value (`--allowedTools a --allowedTools b`) |
| `slots/instructions.rs` | the preamble prepended once to each declared instruction file that exists. A file already led by it is unchanged |
| `slots/hooks.rs` | `HooksAdapter` (`render`, and `try_render`, which an adapter overrides to refuse params it cannot honour), and `ClaudeSettingsLocal` (`claude-settings-local`), which renders `<workdir>/.claude/settings.local.json` with `tddy_agent_backend::build_claude_hooks_settings`. `adapter(name)` returns the adapter, and refuses an unknown name |
| `slots/cursor.rs` | `render_cursor_mcp_json` (compact `{"mcpServers": …}`), `placed_path`, `render_rules_file` (the preamble written whole to `<workdir>/<rules_file>`), and `CursorHooksJson` (`cursor-hooks-json`): `<workdir>/.cursor/hooks.json`, the five Cursor events each running `tddy-tools session-hook --daemon-socket <socket>`. Params without a socket are `NoDaemonSocket`, never a `daemon_url` fallback |
| `specs/claude.yaml` | Claude Code CLI. Its `coding_agent.headless` section renders `claude -p {prompt} --output-format stream-json --verbose [--permission-mode] [--model] [--resume\|--session-id] <slots> <route_args>`, registers MCP with `--mcp-config` and no `--strict-mcp-config` (a headless run keeps the user's own servers), reads through `claude-stream-json` and sets `exit_ok_if_structured: true`. Beside `run:` and `extensions.coding_agent`, it carries the generic jail sections a sandboxed launch lowers: `home: { kind: persistent, key: claude }`, `files:` (credentials seed-once, install mirrors, `CLAUDE_CODE_TMPDIR` / `CLAUDE_TMPDIR` overlays), `secrets:` (`CLAUDE_CODE_OAUTH_TOKEN` from the host env, `when: unrouted`), `policy:` and `network:` (Anthropic API, telemetry and login hosts, `oauth_inbound: true`) — see [Claude CLI session § The jail](../../../docs/ft/daemon/claude-cli-session.md#the-jail-from-the-claude-tool-definition) |
| `specs/cursor.yaml` | Cursor Agent CLI: `agent [--resume {chat_id}] [--model {model}] <slots> [{prompt}]`, or the bundled `node` and `index.js` when the binary is the launcher script; placed `{home}/.cursor/mcp.json`; the managed-workflow `rules_file`; `cursor-hooks-json`; `tool_allow` / `tool_deny` / `permission_prompt` all `unsupported`. Jail sections: `home: { kind: persistent, key: cursor }`, seed-once `~/.cursor/{auth,cli-config}.json`, the install mirrored, `CURSOR_TMPDIR` / `CURSOR_SKIP_KEYCHAIN` / `CI` overlays, `policy:` and `network.egress.hosts` — see [Cursor CLI session § Launch](../../../docs/ft/daemon/cursor-cli-session.md#launch-from-the-cursor-tool-definition) |
| `specs/cursor.yaml` (headless) | `coding_agent.headless`: `agent [--plan] [--resume {session_id}] -p {prompt} [--model] --output-format stream-json --stream-partial-output [--force] --trust --approve-mcps <slots>`; `plan` and `force` are turn params (the workflow backend binds `force`, a conversation does not). `adapter: cursor-stream`; `headless.mcp: { kind: workdir, path: .cursor/mcp.json }`. `models` gains `list_command: ["{binary}", --list-models]` and `adapter: cursor-models` |
| `specs/codex.yaml` | Codex CLI (`codex`, `TDDY_CODEX_CLI`): headless `exec <slots> -C {workdir} -s {sandbox_mode\|workspace-write} --json [resume] <route_args> [-m {model}] [{thread}] {prompt}`, `adapter: codex-jsonl`, `exit_ok_if_structured: true`; MCP `inline` (`-c`, `mcp_servers`); `tool_deny: unsupported`; `placements: [host]` |
| `specs/oz.yaml` | Warp's headless runner (`oz`, `TDDY_OZ_CLI`), distinct from `warp.yaml`: headless `oz agent run -p {prompt} --output-format ndjson -C {workdir} [--conversation] [--model] <slots> [--profile]`, `adapter: oz-ndjson`; MCP `argv` (`--mcp`); tool slots `unsupported`; `placements: [host]` |
| `specs/opencode.yaml` | OpenCode CLI: `opencode` then `--continue` on a resume, else `[--model {model}] [--prompt {prompt}]`; `mcp`, hooks and tool slots `unsupported`; `models: { list_command: ["{binary}", models], adapter: opencode-models }`; `route: { required_in_jail: true, config_env: OPENCODE_CONFIG, files_dir: provider-route }`; `placements: [host, jail]` — see [OpenCode CLI session](../../../docs/ft/daemon/opencode-cli-session.md) |
| `specs/muse.yaml` | Muse Code CLI: `muse` then `--continue` on a resume, else `[--model {model}] [{prompt}]`; the no-JIT jail policy; `route` with `key_env: META_API_KEY`, `required_in_jail: true`; `placements: [host, jail]` — see [Muse CLI session](../../../docs/ft/daemon/muse-cli-session.md) |
| `specs/warp.yaml` | Warp CLI: `warp [--resume {resume_token}]`, no model; `resume: { adapter: warp-resume-token }`; `placements: [host]` (no jail sections) — see [Warp CLI session](../../../docs/ft/daemon/warp-cli-session.md) |
| `lib.rs` fields (`#agent-tools 8/12`) | on `CodingAgentSection`: `route: Option<RouteSlot { required_in_jail, config_env, key_env, files_dir }>`, `models.{list_command, adapter}`, `resume: Option<ResumeSlot { adapter }>`, `placements: Vec<Placement>` (`Placement::anywhere()` by default; `CodingAgentSpec::require_placement`); `McpSlot::Unsupported`; the `unsupported` hooks adapter (`UnsupportedHooks`, `SlotError::HooksUnsupported`); `spec_for_session_type(&str) -> Option<&str>` |

## Rules

- **Nothing is written here.** The renderers return bytes and argv. The host launcher
  (`tddy_agent_launch::spec_launch::launch_host_pty`) writes `RenderedSlots::files` before it spawns;
  the jail launcher (`tddy_agent_launch::jail_launch`) renders the slots for jail paths and hands the
  files to the runner in its exec plan, which writes them inside the jail.
  Only `instructions` reads the disk.
- **`argv_extra` is ordered** as the append-system-prompt flag, the allow flags, the deny flags,
  the permission-prompt flag, then the MCP flags. The template splices it before the positional
  prompt. The MCP `strict_flag` keeps the variadic `--mcp-config` from swallowing that prompt.
- **An `unsupported` slot with values is refused** (`SlotUnsupported`), never dropped. With nothing to
  pass, it renders nothing.
- **The hook protocol is not declarable.** A spec names an adapter, and the adapter is Rust.

Tests: `tests/cursor_spec_acceptance.rs` (`cursor.yaml`'s section, and the registry holding `cursor` beside `claude`); `tests/spec_load_acceptance.rs` (the built-in spec, an unknown slot kind, an unknown extension
owner, an unregistered headless adapter, and their positive controls); `tests/headless_render_golden.rs`
(the built-in headless section renders today's hand-built Claude argv byte for byte). Unit tests cover `slots::tests`, `slots::mcp::tests`,
`slots::hooks::tests`, `slots::cursor::tests` and `spec::tool_slot_tests`.
