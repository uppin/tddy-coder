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
| `spec.rs` (headless) | `HeadlessSection { argv, env, adapter, exit_ok_if_structured, mcp: Option<McpSlot>, system_prompt, goal_params, approval_tool, workflow_mcp, route_model, fresh_session, scratch, permission_policy }` at `coding_agent.headless` (`CodingAgentSpec::headless()`, `run_section()` as a `stdio` run section, `protocol()` → `HeadlessProtocol`). `CodingAgentExtension::validate` refuses an `adapter` that `tddy_agent_backend::adapters::AdapterRegistry::builtin()` does not hold. The protocol fields are what a `SpecHeadlessBackend` workflow turn runs with beyond its argv — see [The headless protocol](#the-headless-protocol). `permission_policy: Option<HeadlessPermissionPolicy>` — who answers a headless conversation's permission requests, spelled `asks_caller` or `{ fixed: <text> }` (the text shown on its catalog row); the built-in `claude` asks its caller, `cursor`, `codex` and `oz` declare a fixed policy. Absent, the agent has no policy and `tddy-coding-subagent` does not offer it (`#agent-tools 27/32`) |
| `headless.rs` | `validate_headless` (`HeadlessSpecError::{NoHeadlessSection, UnknownAdapter}`, naming the adapter and the registered ones). `SpecHeadlessRenderer::new(spec, tool, deps, adapters)` implements the backend's `HeadlessRenderer` port: `slots::render` (no preamble, no hooks; `headless.mcp` overrides `coding_agent.mcp`), then `render_argv` with `prompt`, `model`, `permission_mode`, `workdir`, `session_id` / `resume`, each turn `param`, and the lists `slots` and `route_args`. `tool_server()` is the resolved `tddy-tools` dep (`TOOL_SERVER_DEP`) run with `--mcp`. A turn's allowed tools are not passed to an agent whose `tool_allow` is `unsupported` (it approves under its own mode: Cursor's `--force`, Codex's sandbox). `protocol()` is the section's `HeadlessProtocol`; `model_listing()` renders `coding_agent.models.list_command` with the renderer's resolved binary and the models adapter `models.adapter` names (`None` for a spec with no listing). Re-exports the protocol types (`HeadlessProtocol`, `SystemPromptDelivery`, `GoalParams`, `RouteModel`, `FreshSession`, `ScratchDir`), which live beside the port in `tddy_agent_backend::adapters::headless` |
| `slots/mod.rs` | `SlotInputs` (workdir, scratch dir, MCP server — a `McpServerCommand { name, command, args, env, cwd }`, `cwd` the directory an inline registration starts the server in, `None` for no override — allowed / denied tools, permission-prompt tool, preamble, append-system-prompt file, hook params, and `home` — what `{home}` resolves to: the jail's persistent home or the host's `$HOME`). `render(&CodingAgentSpec, &SlotInputs) -> RenderedSlots { argv_extra, files }`. `SlotError::{UnknownHooksAdapter, Io, SlotUnsupported, NoDaemonSocket, UnboundHome}` |
| `slots/mcp.rs` | a `File` slot: the registration file `<scratch>/claude-mcp-config.json` (`{"mcpServers": {<name>: {command, args, env?}}}`), and `flag <path> [strict_flag]`. A `Placed` slot: the registration at its resolved path, and no argv |
| `slots/headless_mcp.rs` | the headless MCP kinds: `Inline { flag, key_prefix }` renders `-c mcp_servers.<n>.{command,args,env_vars}=…` (`render_inline_mcp`; TOML values; env vars named, never valued, and `env_vars` left out when the server has no env), then `-c mcp_servers.<n>.cwd="<dir>"` (`render_inline_mcp_cwd`) when the server names a `cwd` — how a headless Codex conversation's `tddy-tools` starts in the conversation directory that holds its session pointer, `Argv { flag }` writes a servers-by-name file (`ARGV_MCP_FILE`) owner-only under the scratch dir and renders `--mcp <path>`, `Workdir { path, format }` writes `<workdir>/<path>` (`cursor-mcp-json`). `mcp_written_paths` lists every kind's files, which `RenderedSlots.written` carries so the launcher records them in `headless/<conversation>/written-files` |
| `slots/tools.rs` | one flag per value (`--allowedTools a --allowedTools b`) |
| `slots/instructions.rs` | the preamble prepended once to each declared instruction file that exists. A file already led by it is unchanged |
| `slots/hooks.rs` | `HooksAdapter` (`render`, and `try_render`, which an adapter overrides to refuse params it cannot honour), and `ClaudeSettingsLocal` (`claude-settings-local`), which renders `<workdir>/.claude/settings.local.json` with `tddy_agent_backend::build_claude_hooks_settings`. `adapter(name)` returns the adapter, and refuses an unknown name |
| `slots/cursor.rs` | `render_cursor_mcp_json` (compact `{"mcpServers": …}`), `placed_path`, `render_rules_file` (the preamble written whole to `<workdir>/<rules_file>`), and `CursorHooksJson` (`cursor-hooks-json`): `<workdir>/.cursor/hooks.json`, the five Cursor events each running `tddy-tools session-hook --daemon-socket <socket>`. Params without a socket are `NoDaemonSocket`, never a `daemon_url` fallback |
| `specs/claude.yaml` | Claude Code CLI. Its `coding_agent.headless` section renders `claude -p {prompt} --output-format stream-json --verbose [--permission-mode] [--model] [--resume\|--session-id] <slots> <route_args>`, registers MCP with `--mcp-config` and no `--strict-mcp-config` (a headless run keeps the user's own servers), reads through `claude-stream-json` and sets `exit_ok_if_structured: true`; its protocol is `system_prompt: flag`, `goal_params: { read_only_plan: { permission_mode: plan }, accept_edits: { permission_mode: acceptEdits } }`, `approval_tool` and `workflow_mcp` set, `route_model: { kind: replaced_if_route_has, flag: --model }`, `fresh_session: caller`, `scratch: session_dir`. Its `run.env` declares `CLAUDE_CODE_NO_FLICKER: "1"` (the fullscreen renderer; `#agent-tools 19/19` — no launch path adds it out of band, so an override without it starts Claude inline). Beside `run:` and `extensions.coding_agent`, it carries the generic jail sections a sandboxed launch lowers: `home: { kind: persistent, key: claude }`, `files:` (credentials seed-once, install mirrors, `CLAUDE_CODE_TMPDIR` / `CLAUDE_TMPDIR` overlays), `secrets:` (`CLAUDE_CODE_OAUTH_TOKEN` from the host env, `when: unrouted`), `policy:` and `network:` (Anthropic API, telemetry and login hosts, `oauth_inbound: true`) — see [Claude CLI session § The jail](../../../docs/ft/daemon/claude-cli-session.md#the-jail-from-the-claude-tool-definition) |
| `specs/claude.yaml` (variants) | `coding_agent.variants.host_agent`: `args: [--strict-mcp-config, --setting-sources, user]` and `withdraw_native: [Read, Write, Edit, MultiEdit, NotebookEdit, Bash, BashOutput, KillShell, Glob, Grep, LS]`. It is the agent a split session and `tddy-sandbox-app --codebase-mode sandboxed` run on this host against a codebase that lives elsewhere. The checkout's `.mcp.json` and `.claude/settings.json` hooks cannot configure it, and no native filesystem or shell tool reaches this host's disk. Its launchers clear the MCP slot's `strict_flag`, because the variant already passes it |
| `lib.rs` variants (`#agent-tools 10/12`) | `CodingAgentSection.variants: BTreeMap<String, AgentVariant { args, withdraw_native }>` (`#[serde(default)]`, `deny_unknown_fields`) and `HOST_AGENT_VARIANT`. `CodingAgentSpec::variant_args(name)` returns the variant's `args`, then `tool_deny`'s flag once per `withdraw_native` tool. It refuses an undeclared variant with `VariantError::Unknown`, naming the declared ones, and a variant that withdraws tools through an `unsupported` deny slot with `VariantError::DenyUnsupported` |
| `spec.rs` (ACP, `#agent-tools 11/12`) | `AcpSection { command, env, hooks_adapter, mcp, tool_deny, withdraw_native }` at `coding_agent.acp` (`CodingAgentSpec::{acp, acp_mcp, acp_tool_deny, acp_command}`, `run_section()`); `run.transport: acp`; `McpSlot::AcpSession` (`acp-session`); `ToolSlot::{AcpMeta { acp_meta: <path> }, ConfigEnv { config_env: <VAR> }}`. `CodingAgentExtension::validate` refuses a `hooks_adapter` that `AcpHooksAdapters::builtin()` does not hold. `package_runner(def, host)` binds the param `claude-acp.yaml` uses to run `bunx` only when the binary came from `binary.command` |
| `slots/acp.rs` | `acp_mcp_servers` (the `session/new` `mcp_servers`), `acp_withdrawal -> AcpWithdrawal` (`acp_meta`: the nested `_meta` object listing `withdraw_native`; `config_env`: `{"tools": {<name>: false}}` for that variable) |
| `specs/claude-acp.yaml` | `bunx claude-agent-acp` (`TDDY_CLAUDE_AGENT_ACP_CLI` names a bare binary); MCP `acp-session`; `tool_deny: { acp_meta: claudeCode.options.disallowedTools }` |
| `specs/codex-acp.yaml` | `codex-acp [route_args]`, `binary.sibling_of: codex`, `deps: [codex, tddy-tools]`, `hooks_adapter: codex-acp-login`, `tool_deny: unsupported` (a managed session refuses it) |
| `specs/opencode.yaml` (ACP) | `coding_agent.acp`: `opencode acp`, `hooks_adapter: opencode-route-file`, `tool_deny: { config_env: OPENCODE_CONFIG_CONTENT }`; the PTY sections unchanged |
| `claude_tools.rs` | Claude's tool vocabulary for its `tool_allow` / `tool_deny` slots: `workspace_exec_tool_names` (mirrors `tddy_tool_engine::tool_catalog`), `build_claude_allowlist(subagent_enabled, replaced)`, `build_claude_disallowlist(replaced)`, `effective_replaced_tools` and `PERMISSION_PROMPT_TOOL`. It lives with Claude's spec because the jail crates name no agent |
| `specs/cursor.yaml` | Cursor Agent CLI: `agent [--resume {chat_id}] [--model {model}] <slots> [{prompt}]`, or the bundled `node` and `index.js` when the binary is the launcher script; placed `{home}/.cursor/mcp.json`; the managed-workflow `rules_file`; `cursor-hooks-json`; `tool_allow` / `tool_deny` / `permission_prompt` all `unsupported`. Jail sections: `home: { kind: persistent, key: cursor }`, seed-once `~/.cursor/{auth,cli-config}.json`, the install mirrored, `CURSOR_TMPDIR` / `CURSOR_SKIP_KEYCHAIN` / `CI` overlays, `policy:` and `network.egress.hosts` — see [Cursor CLI session § Launch](../../../docs/ft/daemon/cursor-cli-session.md#launch-from-the-cursor-tool-definition) |
| `specs/cursor.yaml` (headless) | `coding_agent.headless`: `agent [--plan] [--resume {session_id}] -p {prompt} [--model] --output-format stream-json --stream-partial-output [--force] --trust --approve-mcps <slots>`; `plan` and `force` are turn params (the workflow backend binds `force`, a conversation does not). `adapter: cursor-stream`; `headless.mcp: { kind: workdir, path: .cursor/mcp.json }`. `models` gains `list_command: ["{binary}", --list-models]` and `adapter: cursor-models`. Protocol: `system_prompt: prepend`, `goal_params: { always: { force: "1" }, plan_mode: { plan: "1" } }`, `workflow_mcp`, `fresh_session: agent`, `scratch: workdir` (its MCP file lies in the worktree) |
| `specs/codex.yaml` | Codex CLI (`codex`, `TDDY_CODEX_CLI`): headless `exec <slots> -C {workdir} -s {sandbox_mode\|workspace-write} --json [resume] <route_args> [-m {model}] [{thread}] {prompt}`, `adapter: codex-jsonl`, `exit_ok_if_structured: true`; `headless.env` `BROWSER: "{self_exe}"` and `TDDY_CODEX_OAUTH_OUT: "{session_dir}/codex_oauth_authorize.url"` (its OAuth capture: the one `TDDY_*` variable a headless agent sees); protocol `system_prompt: prepend`, `goal_params: { read_only_plan: { sandbox_mode: read-only } }`, `route_model: { kind: replaced }`, `fresh_session: agent`, `scratch: session_dir`; MCP `inline` (`-c`, `mcp_servers`); `tool_deny: unsupported`; `placements: [host]` |
| `specs/oz.yaml` | Warp's headless runner (`oz`, `TDDY_OZ_CLI`), distinct from `warp.yaml`: headless `oz agent run -p {prompt} --output-format ndjson -C {workdir} [--conversation] [--model] <slots> [--profile]`, `adapter: oz-ndjson`; protocol `system_prompt: prepend`, `workflow_mcp`, `fresh_session: agent`, `scratch: session_dir`; MCP `argv` (`--mcp`); tool slots `unsupported`; `placements: [host]` |
| `specs/opencode.yaml` | OpenCode CLI: `opencode` then `--continue` on a resume, else `[--model {model}] [--prompt {prompt}]`; `mcp`, hooks and tool slots `unsupported`; `models: { list_command: ["{binary}", models], adapter: opencode-models }`; `route: { required_in_jail: true, providers: [openai_responses, openai_chat], shape: json_config_file, config_env: OPENCODE_CONFIG, files_dir: provider-route, key_env: OPENCODE_PROVIDER_KEY }`; `placements: [host, jail]` — see [OpenCode CLI session](../../../docs/ft/daemon/opencode-cli-session.md) |
| `specs/muse.yaml` | Muse Code CLI: `muse` then `--continue` on a resume, else `[--model {model}] [{prompt}]`; the no-JIT jail policy; `route` with `key_env: META_API_KEY`, `required_in_jail: true`; `placements: [host, jail]` — see [Muse CLI session](../../../docs/ft/daemon/muse-cli-session.md) |
| `specs/warp.yaml` | Warp CLI: `warp [--resume {resume_token}]`, no model; `resume: { adapter: warp-resume-token }`; `placements: [host]` (no jail sections) — see [Warp CLI session](../../../docs/ft/daemon/warp-cli-session.md) |
| `registry.rs` (`#agent-tools 12/19`) | `CodingAgentRegistry`, the one enumeration of the agents a host offers: `load(overrides_dir)` (`load_agent_registry`, then `from_tools`), `from_tools(ToolRegistry)` (every tool with a `coding_agent` extension, through `CodingAgentSpec::of`; any invalid one refuses the registry), `builtin()`, `empty()`, `tools()`, `all()` (ordered by id), `get(id)` and `for_session_type(st)`: `<id>-cli` names agent `<id>` when its `run.transport` is `pty` (`CLI_SESSION_TYPE_SUFFIX`), so a dropped-in `acme.yaml` gets `acme-cli`; `None` for `tool`, `workspace`, a bare id and the `-cli` of an ACP-only agent. `RegistryError::{Tools, Spec}` |
| `capability.rs` (`#agent-tools 12/19`) | `Capability::{Hooks, ManagedWorkflow, AskUserQuestion, SplitHost, ProjectHomeMain, RequiresModel, BranchNameRequired, KeepWorktreeOnDelete, OperatorNotify}` (closed, snake_case in YAML, `ALL`, `as_str`). `CodingAgentSection.capabilities: BTreeSet<Capability>`; `CodingAgentSpec::{capabilities, has}`. An unknown word is refused at load, naming the file and the word. `managed_workflow` admits a managed-codebase start of the agent's CLI session (`tddy_agent_launch::start_request_checks::refuse_an_unsupported_cli_session_start`). Built-ins: `claude` declares hooks, managed_workflow, ask_user_question, split_host, project_home_main, requires_model, branch_name_required and operator_notify; `cursor` hooks, managed_workflow, requires_model, keep_worktree_on_delete and operator_notify; every other agent none |
| `slots/conversation.rs` (`#agent-tools 14/19`) | `coding_agent.conversation`: `ConversationSlot`, serde-tagged `{ kind: daemon_session_id \| minted \| worktree \| resume_token }`. `daemon_session_id` binds the daemon's session id as `session_id` (claude: `--session-id` on a start, `--resume` on a resume); `minted { adapter, mint_deadline_secs }` runs a `tddy_agent_backend::adapters::LaunchAdapters` conversation-id adapter (`cursor-create-chat`) before the first start, records the id on `.session.yaml` `cursor_chat_id` and binds it as `chat_id` on every launch; `mint_deadline()` is `mint_deadline_secs` or `DEFAULT_CONVERSATION_MINT_DEADLINE_SECS` (30); `worktree` binds nothing (`--continue`); `resume_token` binds the token `coding_agent.resume.adapter` read. `CodingAgentSpec::conversation()` derives `resume_token` (with a `resume` slot) or `worktree` when absent, so opencode, muse and warp declare none. `CodingAgentExtension::validate` refuses an adapter the build does not hold and `resume_token` without a `resume` slot, naming the file. Built-ins: `claude` `daemon_session_id`, `cursor` `minted` by `cursor-create-chat` |
| `route.rs` (`#agent-tools 12/19`, 13/19) | a re-export of `tddy_agent_backend::provider_route::{ProviderKind, RouteShape, RouteSupport, RoutedAgent}`: the types live beside the route bodies, since this crate depends on that one. `ProviderKind::{Anthropic, OpenaiResponses, OpenaiChat, Meta}`, `RouteShape::{Env (default), TomlArgs, JsonConfigFile}` (YAML `env`/`toml_args`/`json_config_file`, named after the body format; a removed `args` or `config_file` is refused at load naming its replacement), `RouteSupport { providers, shape }` (`is_routable`). Read from `coding_agent.route.{providers, shape}` (`RouteSlot`) by `CodingAgentSpec::route_support()`; no `route` section is not routable. Built-ins: `claude` and `claude-acp` `[anthropic]`/env, `codex` and `codex-acp` `[openai_responses]`/toml_args with `key_env: CODEX_PROVIDER_KEY`, `opencode` `[openai_responses, openai_chat]`/json_config_file with `key_env: OPENCODE_PROVIDER_KEY`, `muse` `[meta]`/env with `key_env: META_API_KEY`. No route `key_env` is a `TDDY_*` name. The route body is chosen by the spec's (shape, provider served) — see `tddy-agent-backend` [Provider routes](../../tddy-agent-backend/docs/architecture.md#provider-routes-provider_route). `tests/route_body_drift_acceptance.rs` pins each built-in's providers against `bodies_accept(shape)`, every built-in's `product`, and that no built-in route hands the agent a `TDDY_*` variable |
| `spec.rs` product and `routed_agent` (`#agent-tools 13/19`) | `coding_agent.product: Option<String>`, the operator-facing name a route refusal leads with (`Claude Code`, `Codex`, `Muse Code`), declared by every built-in; `CodingAgentSpec::product()` answers the id when absent. `routed_agent(&CodingAgentSpec) -> RoutedAgent { id, product, support, key_env }` is what a route body knows of the agent |
| `spec.rs` answer wait (`#agent-tools 12/19`) | `coding_agent.answer_wait_secs: Option<u64>`, read by `CodingAgentSpec::answer_wait_secs()` (`DEFAULT_ANSWER_WAIT_SECS` = 120 when absent): how long an orchestrated session's permission hook waits for its orchestrator's answer. Since `#agent-tools 19/19` it is the only source: the launch writes it into the hooks file (`timeout = wait + 10`) and records it beside the session, and `AwaitQuestionAnswer` holds for the recorded value |
| `lib.rs` fields (`#agent-tools 8/12`) | on `CodingAgentSection`: `route: Option<RouteSlot { required_in_jail, config_env, key_env, files_dir }>`, `models.{list_command, adapter}`, `resume: Option<ResumeSlot { adapter }>`, `placements: Vec<Placement>` (`Placement::anywhere()` by default; `CodingAgentSpec::require_placement`); `McpSlot::Unsupported`; the `unsupported` hooks adapter (`UnsupportedHooks`, `SlotError::HooksUnsupported`); a CLI session type resolves to its spec through `CodingAgentRegistry::for_session_type` |

## The only home of agent metadata (the sweep, `#agent-tools` 20/28)

Since the sweep (PR #828), a spec is the **only** place an agent's metadata lives. The legacy
Rust descriptor table (`CliHarness`, `claude_argv`, the curated-model and context-glob tables,
the bespoke ACP backends) is deleted; nothing in production names an agent's binary, argv,
models or capabilities outside `specs/*.yaml`. Protocol adapters (stream parsers, hook-file
writers) stay as Rust, selected by the name a spec gives them; they own no metadata.

| Metadata | Spec home |
|---|---|
| session type, label/product | `id` (`<id>-cli` for a PTY agent), `coding_agent.product` |
| binary, env override, route config env | tool `binary`, `coding_agent.route.config_env` |
| CLI argv (PTY + headless) | `run.argv`, `coding_agent.headless.argv` |
| models (curated / listed) | `coding_agent.models` (`CuratedModel`: a bare id, or `{ id, label }`) |
| context globs, instructions files | `coding_agent.context_globs`, `instructions.files` |
| requires-model, placements, capabilities | `coding_agent.capabilities`, `placements` |
| permission modes, transcript usage | `coding_agent.permission_modes`, `coding_agent.transcript_usage` |

The argv templates double as the CLI-arg schema: `CodingAgentSpec::argv_params()` is the set of
params a spec's `run.argv` binds, `binds_argv_param(param)` answers for one, `render_run_argv`
renders a launch from bound values, and `model_listing_argv()` / `curated_models()` answer the
model catalogue. `CodingAgentRegistry::builtin_cached()` is the process-wide builtin view for
consumers that held only the old static table.

**No package depends on this crate to learn about an agent.** Specs are injected as plain data
by binary and composition-root modules — `AgentChoice` into `tddy-tool-engine`, a
`default_models` map into `tddy-presenter`, `[AgentModels]` into `tddy-tools`, guidance files
and globs as values into sandbox and session code. The only crate that depends on this one for
agent metadata is `tddy-tools-cli`, a binary crate.

The web bundle reads the same metadata off the wire, never from a table of its own: the serving
daemon's `GET /api/config` (`ClientConfig.allowed_agents`) for its own selection, and
`CatalogService.ListAgents` per target host. `AgentInfo` carries the spec-derived fields
(`session_type`, `capabilities`, `run_params`, `permission_modes`, `default_model`,
`placements`) beside `id` and `label`.

## Which transport a workflow turn takes

`tddy-coder --agent <id>` builds a workflow backend for any agent in the registry
(`tddy_coder::spec_backend_build::build_spec_backend`, `#agent-tools 16/19`). The spec's sections
decide how it takes a turn (`tddy_agent_backend::SpecTransport::chosen`):

| The spec declares | Nothing asked | ACP asked | Headless asked |
|---|---|---|---|
| `coding_agent.headless` and `coding_agent.acp` | **headless** | ACP | headless |
| only `coding_agent.headless` | headless | refused (`TransportNotDeclared`) | headless |
| only `coding_agent.acp` | ACP | ACP | refused (`TransportNotDeclared`) |
| neither (a PTY-only agent: `muse`, `warp`) | refused (`NoTurnTransport`) | refused | refused |

**Headless wins over ACP** when a spec declares both, unless the session or the request asks for
ACP (`SpecBackendRequest.transport`). Every refusal names the agent. No agent ever falls back to
another, and an id no agent answers to is refused listing the ids that can run a turn
(`agent_registry_cli::backend_agent_ids`). A YAML-only ACP agent runs through the generic
`SpecAcpBackend`: its rendered `acp.command`, the hooks adapter it names and that adapter's deps.

## The headless protocol

A workflow turn of a headless agent (`tddy_agent_backend::SpecHeadlessBackend`) differs between
agents only by these `coding_agent.headless` fields, each named for what it does. An unknown word in
any of them is refused at load, naming the file and the word.

| Field | Values | Absent |
|---|---|---|
| `system_prompt` | `flag` — a file, through the `append_system_prompt` slot; `prepend` — in front of the prompt, after a blank line | `prepend` |
| `goal_params` | maps of template param to value: `always`, `plan_mode` (a goal in the agent's plan mode), `read_only_plan` (also read-only), `accept_edits` (a goal that accepts edits), bound in that order, a later one overriding a name; a param named `permission_mode` fills `{permission_mode}` | none |
| `approval_tool` | bind `mcp__tddy-tools__approval_prompt` as the permission-prompt tool when the tool server is registered | `false` |
| `workflow_mcp` | register the agent's tool server (its `tddy-tools` dep with `--mcp`) | `false` |
| `route_model` | `{ kind: kept }`, `{ kind: replaced }` (a routed turn names no model; the route's argv does), `{ kind: replaced_if_route_has, flag: <flag> }` | `kept` |
| `fresh_session` | `caller` — the caller's id is passed as `session_id`; `agent` — the agent mints one, read off its stream | `agent` |
| `scratch` | where the turn's registration files go: `session_dir` or `workdir` | `session_dir` |

`headless.env` values may name `{session_dir}` (the request's session directory; a turn without one
is refused) and `{self_exe}` (the running executable). The backend strips every inherited `TDDY_*`
variable before it adds them. A YAML file under `<tddyhome>/tools` that declares a
`coding_agent.headless` section naming a registered adapter runs a workflow turn with no Rust change.

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
(the built-in headless section renders Claude's argv byte for byte); `tests/headless_protocol_spec_unit.rs` (each built-in's protocol, the defaults, Cursor's model listing through its spec-resolved binary, an unknown `system_prompt`, `route_model`, `fresh_session`, `scratch` or `goal_params` word refused at load). Unit tests cover `slots::tests`, `slots::mcp::tests`,
`slots::hooks::tests`, `slots::cursor::tests` and `spec::tool_slot_tests`. `tests/agent_registry_unit.rs` covers the registry (every built-in by id, `for_session_type`, the built-ins' capabilities, `has`, an unknown capability refused, `route_support` with and without a `route` section). `tests/cli_spec_features_unit.rs` covers `managed_workflow` and the conversation slot (each kind, the derived default, the mint deadline, an unknown adapter and a `resume_token` without `resume` refused at load).
