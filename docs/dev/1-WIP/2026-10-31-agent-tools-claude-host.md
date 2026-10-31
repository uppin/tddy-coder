# Changeset: Claude Code CLI launches from its tool definition

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — four crates (`tddy-actions`, new `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-lifecycle`); a new contract (argv template + extensions + mapping slots) every agent node builds on; byte-identical output pinned against today's launch; the template grammar is new design
**Stack:** `#agent-tools 2/12` — branch `feature/agent-tools/claude-host`, base `feature/agent-tools/tool-defs`
**PRD:** [PRD-2026-10-31-claude-cli-from-spec.md](../../ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-from-spec.md)

## Initial Discovery

[2026-10-31-agent-tools-claude-host-initial-discovery.md](./2026-10-31-agent-tools-claude-host-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-24-claude-agent-tab-shares-session-hooks.md](../todo/2026-10-24-claude-agent-tab-shares-session-hooks.md) | ⚠ During. The `hooks` slot renders one session's hooks, and agent tabs are untouched. Keep the slot renderer parameterised by an activity identity, so the fix this entry describes (per-tab `--settings`) becomes a slot parameter later rather than a second writer |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | ⚠ During. Resume moves onto the launcher, and `resume_claude_cli_session` must shrink, not grow. Promote to ✅ at wrap if the measurement says so |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ During. `start_here`'s claude-cli arm calls the launcher and gains no branches |
| `tddy-coding-agent-specs` (new) | ℹ A new crate starts inside the file and function budgets |

## Affected Packages

- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `ToolDef.run` (argv template, env, transport) and `ToolDef.extensions` (open, owner-validated); template rendering; built-in `tools/tddy-tools.yaml`
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): **new crate**. Embedded `specs/claude.yaml`; `CodingAgentSpec` (validates the `coding_agent` extension); slot renderers for MCP file, allow/deny argv, permission prompt, instruction files and hooks adapter name
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): spec-driven host PTY launcher; the unsandboxed claude-cli start and resume use it
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): claude-cli spawn wiring passes the spec registry through
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): registers the `coding_agent` extension owner on the registry, and passes daemon.yaml `claude_cli.binary_path` as the binary override until #12

## Responsibility

- `tddy_actions::tool::{RunSection { transport: Transport, argv: Vec<ArgvItem>, env: BTreeMap<String,String> }, Transport::{Pty, Stdio}, ArgvItem::{Lit(String), When { if_: String, then: Vec<ArgvItem>, else_: Vec<ArgvItem> }}}`. `{param}` placeholders appear inside literals, and `{binary}` is always bound.
- `tddy_actions::tool::render_argv(&RunSection, &ResolvedTool, &TemplateParams) -> Result<Vec<String>, ToolError>`. An unbound placeholder is an error naming it, never an empty string.
- `ToolDef.extensions: BTreeMap<String, serde_yaml::Value>` and `ToolRegistry::load_with_owners(…, &[&dyn ExtensionOwner])`. An unknown owner, or an owner that rejects its section, fails the load and names the file.
- `packages/tddy-actions/tools/tddy-tools.yaml`, a built-in tool that coding agents list in `deps`.
- Crate `tddy-coding-agent-specs`: `builtin_specs() -> Vec<(path, &str)>`, `CodingAgentExtension` (the `ExtensionOwner` for `coding_agent`), `CodingAgentSpec::of(&ToolDef) -> Result<CodingAgentSpec>`, with sections `mcp: McpSlot { kind: File { flag, format } }`, `tool_allow`, `tool_deny`, `permission_prompt`, `instructions { files, append_system_prompt }`, `hooks { adapter }`, `models { default, curated }` and `context_globs`.
- `tddy_coding_agent_specs::slots::render(&CodingAgentSpec, &SlotInputs) -> RenderedSlots { argv_extra, files: Vec<(PathBuf, Vec<u8>)> }`, where `SlotInputs` carries the MCP server command, the allowed and denied tools, and the preamble.
- `specs/claude.yaml`, as in the PRD.
- `tddy_agent_launch::spec_launch::launch_host_pty(spec, params, slots, worktree, session) -> Result<…>`. The unsandboxed claude-cli start (`start_claude_cli_from_request`) and `resume_claude_cli_session` call it.

## Boundaries

- No jail (#5). The sandboxed claude-cli path still uses today's code.
- No `network:` section (#3, used by #5). No `home`, `files`, `secrets` or `policy` (#5).
- No headless (#6), split sessions or `tddy-sandbox-app` (#10).
- The `claude-cli` session-type string gates are untouched (#12).
- The hook **protocol** (`tddy-tools session-hook`, the PermissionRequest decision JSON) is not changed. Only who writes `.claude/settings.local.json` moves, into the slot renderer.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-defs (#790) | `ToolDef`, `ToolRegistry::load`, `<tddyhome>/tools` overrides, `ToolHost` and `resolve_closure`, in `packages/tddy-actions/src/tool/` | `claude.yaml` loads through the registry, and `claude` plus its `tddy-tools` dep resolve on the host | change the replace-whole override rule, the resolution order, or `ResolvedTool`'s shape; touch `rg.yaml` or `tool_grants` |

## Draft PR contract

The first push publishes:

- the `RunSection`, `ArgvItem` and `Transport` types and `render_argv`;
- `extensions` and `ExtensionOwner`;
- the crate `tddy-coding-agent-specs` with `CodingAgentSpec`, `McpSlot` and the slots `render` signature;
- `claude.yaml` and `tddy-tools.yaml`;
- the `launch_host_pty` signature;
- the failing tests below.

Unimplemented bodies are `// TODO(claude-host): implement`. The claude-cli start keeps today's code until green.

## Green wave

**Wave:** 2 of 7
**Greenable independently:** yes, once #1 is green. Every test uses #1's registry and resolution (behaviour) and a stub `claude` binary on a fixture PATH
**Concurrent with:** #3 egress-allowlist and #4 build-tools
**Blocks:** #5 claude-jail (reuses the launcher's rendering and slots), #6 claude-headless (the `coding_agent` extension and `claude.yaml`), and transitively #7–#12

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- Unsandboxed claude-cli argv is built in `tddy-agent-backend/src/claude_argv.rs` and `tddy-cli-sessions` `argv.rs`. The binary is resolved by `tddy-daemon-kernel` `config/agent_cli.rs` (`claude_cli.binary_path`, `TDDY_CLAUDE_BINARY`, `~/.local/bin`, skipping `.superset/bin`, canonicalized).
- Hooks are written by `claude_hooks.rs` and `agent-launch` `claude_cli_spawn_steps.rs:168`. The MCP config and allow/deny lists come from recipes and from `build_claude_allowlist`. The preamble is `tddy_sandbox::MANAGED_CODEBASE_PREAMBLE`.
- Models and the default `opus` are in `tddy-agent-backend/src/backend/mod.rs:637-741`. Context globs are in `context_globs.rs:37`.
- `ToolDef` (#1) has no `run`, and no extension mechanism.

## State B

- The daemon builds `ToolRegistry::load_with_owners(builtin + agent specs, <tddyhome>/tools, [CodingAgentExtension])`.
- An unsandboxed claude-cli start resolves the `claude` spec plus its closure. It then renders argv from `run.argv` with `{model, session_id, resume, permission_mode, prompt}`, renders the slots (MCP file, allow/deny, permission prompt, instruction files, hooks settings) and spawns through `launch_host_pty`.
- The output is the same argv, env and files as State A, pinned by golden tests.

## Delta

### New
- `packages/tddy-coding-agent-specs/{Cargo.toml, README.md, src/{lib.rs, spec.rs, slots/{mod.rs, mcp.rs, tools.rs, instructions.rs, hooks.rs}}, specs/claude.yaml}`
- `packages/tddy-actions/src/tool/{run.rs, template.rs, extensions.rs}` and `packages/tddy-actions/tools/tddy-tools.yaml`
- `packages/tddy-agent-launch/src/spec_launch.rs`
### Modified
- The root `Cargo.toml` workspace members gain the new crate.
- `claude_cli_spawn*` and `svc_resume_claude_cli_session.rs` call `launch_host_pty`.
- `tddy-daemon` runtime: owner registration, and the binary override is carried from daemon.yaml.
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md` § Launch; `docs/ft/coder/sandbox-builder.md` § Declared tools (run, extensions); the new crate's `docs/`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_from_spec_acceptance.rs`**

- [ ] `an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` (a stub `claude` records argv, env and cwd)
- [ ] `a_claude_cli_resume_renders_resume_and_the_session_id_from_the_same_template`
- [ ] `the_mcp_config_allow_deny_flags_permission_prompt_and_preamble_equal_todays_for_the_same_session` (golden files captured from `master`)
- [ ] `a_tddyhome_override_of_claude_yaml_adds_an_argv_token_that_reaches_the_process`
- [ ] `the_daemon_yaml_claude_binary_path_still_overrides_the_spec_binary`

**`tddy-coding-agent-specs`: `packages/tddy-coding-agent-specs/tests/spec_load_acceptance.rs`**

- [ ] `the_builtin_claude_spec_loads_and_depends_on_tddy_tools`
- [ ] `an_unknown_slot_kind_is_refused_naming_the_file`
- [ ] `an_unknown_extension_owner_is_refused_naming_the_owner`
- [ ] `a_spec_whose_transport_is_not_pty_is_refused_by_the_host_pty_launcher`

### Unit / integration tests

- [ ] `tddy-actions` `tool::template`: literals; placeholders inside literals; `When` with `then` and `else` branches; nested `When`; an unbound placeholder is an error naming it; `{binary}` is the resolved path.
- [ ] `tddy-coding-agent-specs` `slots`: the MCP file has the `claude-mcp-json` shape for a given server command; allow/deny render one flag per list joined as Claude expects; instructions prepend the preamble once (idempotent on a re-run); the hooks adapter name is resolved to a renderer, and an unknown name is refused.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Template grammar, slots | Unit | Pure functions; every agent node reuses them |
| Launch output equals today's | Integration with a stub binary and golden files from `master` | The refactor must be behaviour-preserving, and a golden file is the proof |

## Decisions & Trade-offs

- **Open extensions, owner-validated**, the same model as BUILD.yaml's open `config`. `tddy-actions` stays agent-agnostic, and `tddy-coding-agent-specs` owns the `coding_agent` vocabulary.
- **A structured template (`When` nodes), not a string mini-language.** It is reviewable and serde-checked, and it cannot inject shell.
- **daemon.yaml `claude_cli.binary_path` stays as an override until #12**, so this node changes no configuration.
- **`tddy-tools` is a tool, and agents depend on it.** The relay binary is found the same way as every other binary.
- Complexity 5 because the template and slot grammar is new design that eleven nodes inherit. That is the reason the node owns only Claude on the host.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-claude-host-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
