# Changeset: Claude Code CLI launches from its tool definition

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — four crates (`tddy-actions`, new `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-session-lifecycle`); a new contract (argv template + extensions + mapping slots) every agent node builds on; byte-identical output pinned against today's launch; the template grammar is new design
**Stack:** `#agent-tools 2/12` — branch `feature/agent-tools/claude-host`, base `feature/agent-tools/tool-defs`
**PR:** [#791](https://github.com/uppin/tddy-coder/pull/791)
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
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): registers the `coding_agent` extension owner on the registry, and passes daemon.yaml `claude_cli.binary_path` as the binary override until #801

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

- No jail (#794). The sandboxed claude-cli path still uses today's code.
- No `network:` section (#792, used by #794). No `home`, `files`, `secrets` or `policy` (#794).
- No headless (#795), split sessions or `tddy-sandbox-app` (#799).
- The `claude-cli` session-type string gates are untouched (#801).
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
**Greenable independently:** yes, once #790 is green. Every test uses #790's registry and resolution (behaviour) and a stub `claude` binary on a fixture PATH
**Concurrent with:** #792 egress-allowlist and #793 build-tools
**Blocks:** #794 claude-jail (reuses the launcher's rendering and slots), #795 claude-headless (the `coding_agent` extension and `claude.yaml`), and transitively #796–#801

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- Unsandboxed claude-cli argv is built in `tddy-agent-backend/src/claude_argv.rs` and `tddy-cli-sessions` `argv.rs`. The binary is resolved by `tddy-daemon-kernel` `config/agent_cli.rs` (`claude_cli.binary_path`, `TDDY_CLAUDE_BINARY`, `~/.local/bin`, skipping `.superset/bin`, canonicalized).
- Hooks are written by `claude_hooks.rs` and `agent-launch` `claude_cli_spawn_steps.rs:168`. The MCP config and allow/deny lists come from recipes and from `build_claude_allowlist`. The preamble is `tddy_sandbox::MANAGED_CODEBASE_PREAMBLE`.
- Models and the default `opus` are in `tddy-agent-backend/src/backend/mod.rs:637-741`. Context globs are in `context_globs.rs:37`.
- `ToolDef` (#790) has no `run`, and no extension mechanism.

## State B

- The daemon builds `ToolRegistry::load_with_owners(builtin + agent specs, <tddyhome>/tools, [CodingAgentExtension])`.
- An unsandboxed claude-cli start resolves the `claude` spec plus its closure. It then renders argv from `run.argv` with `{model, session_id, resume, permission_mode, prompt}`, renders the slots (MCP file, allow/deny, permission prompt, instruction files, hooks settings) and spawns through `launch_host_pty`.
- The output is the same argv, env and files as State A, pinned by golden tests.

## Delta

### New
- `packages/tddy-coding-agent-specs/{Cargo.toml, README.md, src/{lib.rs, spec.rs, slots/{mod.rs, mcp.rs, tools.rs, instructions.rs, hooks.rs}}, specs/claude.yaml}`
- `packages/tddy-actions/src/tool/{run.rs, template.rs, extensions.rs}` and `packages/tddy-actions/tools/tddy-tools.yaml`
- `packages/tddy-agent-launch/src/spec_launch.rs` (`HostCodingAgents`, `HostAgent`, `HostPtySession`, `SpecLaunchError`, `launch_host_pty`)
- `packages/tddy-session-lifecycle/src/connection_service/svc_coding_agents_wiring.rs` (`DaemonSessionHost::with_coding_agents`)
- Tests: `packages/tddy-coding-agent-specs/tests/spec_load_acceptance.rs`, `packages/tddy-agent-launch/tests/spec_launch_acceptance.rs`, `packages/tddy-session-lifecycle/tests/claude_cli_from_spec_acceptance.rs` + `tests/fixtures/claude_cli_from_spec/`
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
- [ ] `the_same_override_without_the_unowned_key_loads` (positive control for the owner refusal)

**`tddy-agent-launch`: `packages/tddy-agent-launch/tests/spec_launch_acceptance.rs`** (moved here from `spec_load_acceptance.rs`: the launcher lives in `tddy-agent-launch`, which `tddy-coding-agent-specs` must not depend on)

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
- **daemon.yaml `claude_cli.binary_path` stays as an override until #801**, so this node changes no configuration.
- **`tddy-tools` is a tool, and agents depend on it.** The relay binary is found the same way as every other binary.
- Complexity 5 because the template and slot grammar is new design that eleven nodes inherit. That is the reason the node owns only Claude on the host.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-claude-host-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-10-31: pass 2 runs without per-node review
- [x] TDD Red — write failing unit/integration tests
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

- **Contract amendment (commit 3, 2026-10-31), forced by #796 cursor-cli's red phase.** `tool_allow`, `tool_deny` and `permission_prompt` were each a required `FlagSlot`. That left no way to declare an agent that has no such flag (Cursor, and later Codex, Oz and codex-acp), so `cursor.yaml` could not load.
  - The fields are now `ToolSlot = Flag(FlagSlot) | unsupported`. In YAML the second form is the word `unsupported`.
  - `FlagSlot` is unchanged. `claude.yaml` is unchanged; it still reads `{ flag: … }`.
  - `slots::render` refuses a non-empty list for an `unsupported` slot with `SlotError::SlotUnsupported { agent, slot }` and never drops it.
  - #800 acp-agents extends `ToolSlot` additively, with its `acp_meta` / `config_env` kinds.
  - New tests: `spec::tool_slot_tests` (3 shape pins, which pass — serde does the work), plus `slots::tests::denied_tools_for_an_agent_whose_tool_deny_is_unsupported_are_refused` and its positive control `an_unsupported_tool_deny_with_nothing_to_withdraw_renders_no_deny_flag` (both fail on `render`'s TODO).

**Red baseline (2026-10-31, contract commit).** Every new test compiles and fails on a `todo!` of
`claude-host`; none fails on a parent's TODO first.

| Suite | Tests | Fails at |
|---|---|---|
| `tddy-actions` `tool::template::tests` (lib) | 9 | `render_argv` (`todo!("claude-host: render_argv")`) |
| `tddy-coding-agent-specs` `slots::tests` (lib) | 6 | `slots::render` |
| `tddy-coding-agent-specs` `slots::hooks::tests` (lib) | 2 | `hooks::adapter` |
| `tddy-coding-agent-specs` `tests/spec_load_acceptance.rs` | 4 | `load_agent_registry` |
| `tddy-agent-launch` `tests/spec_launch_acceptance.rs` | 1 | `CodingAgentSpec::of` |
| `tddy-session-lifecycle` `tests/claude_cli_from_spec_acceptance.rs` | 5 | `load_agent_registry` |

Green needs #790 green as well: `load_agent_registry` is built on `ToolDef::parse`, `ToolRegistry::load`
and `resolve_closure`, which are `TODO(tool-defs)` today. That is the planned sequencing (wave 2), not
a new edge.

**Pre-existing failures in touched packages.** The `tddy-actions` suites `tool_registry_acceptance`
(9) and `tool_resolve_unit` (7) fail on #790's own `TODO(tool-defs)` bodies — the parent's red, not
this node's. Environmental failures seen locally, unrelated to this node (the hand-built
path is untouched): `tddy-agent-launch` `clone_askpass_acceptance` (9 — `target/debug/tddy-askpass`
not built by a bare `cargo test`); `tddy-session-lifecycle` `claude_cli_fullscreen_renderer` — 2 host
`fullscreen: false` cases (the runner, itself a Claude Code session, exports
`CLAUDE_CODE_NO_FLICKER=1`, which the suite's doc says it assumes absent) and 3 macOS sandbox cases
(`tddy-sandbox-runner` not built). Baseline note: the step-2 `cargo check` baseline was taken after
this node's first edits had started, so it is not a clean "before"; the existing suites were run on
the final tree instead.

**Goldens.** `packages/tddy-session-lifecycle/tests/fixtures/claude_cli_from_spec/{host_start_argv.txt,
host_start_env.txt, host_start_settings_local.json}` were captured from today's hand-built launch (the
`claude_cli.binary_path` route, no coding agents installed) with a throwaway test that was removed
before the commit. Per-run values are `<SESSION_ID>`, `<WORKTREE>`, `<SESSIONS>`, `<STUBS>`,
`<HOOK_TOKEN>`, `<OS_USER>`. The env golden pins only what the launch sets (`CLAUDE_CODE_NO_FLICKER`,
`TDDY_DAEMON_URL`, `TDDY_HOOK_TOKEN`, `TDDY_OS_USER`, `TDDY_SESSION_ID`) — a host agent inherits
the daemon's environment, so anything else is the test runner's. On the host today the MCP, allow,
deny, permission-prompt and preamble slots are all empty for an ordinary session; the golden pins
that, plus the hooks file and the append-system-prompt flag.

**Deviations from the planned surface (all within this node's own design):**

- **`ArgvItem::Splice { splice }`** — a third template item that expands a list parameter in place.
  The slot flags must sit *between* the permission flags and the positional prompt (`--mcp-config`
  is variadic and swallows a bare positional), and a single-entry literal cannot carry a list.
  `claude.yaml` has `{ splice: slots }` before `{ if: prompt, … }`; `TemplateParams::with_list`
  binds it. An unbound list is `ToolError::UnboundPlaceholder` like an unbound placeholder.
- **`claude.yaml` permission arm** — `{ if: dangerously_skip_permissions, then:
  [--dangerously-skip-permissions], else: [--permission-mode, "{permission_mode}"] }` instead of the
  PRD sketch's optional `--permission-mode`: today's argv always carries one of the two (mode
  defaulted to `auto`), and byte-identity is the contract. Likewise `--model` is under
  `{ if: model }` because today omits it when empty.
- **`claude.yaml` has `env: {}`, not `CLAUDE_CODE_NO_FLICKER: "1"`.** daemon.yaml
  `claude_cli.fullscreen: false` turns that variable off today (`claude_cli_renderer_env`); a static
  spec env would make it unconditional and break `claude_cli_fullscreen_renderer.rs`. It stays in
  the launcher's env until the fullscreen switch is a template parameter or moves (decide at green).
- **`BUILTIN_SUPPORT_TOOLS`** (new const in `tddy-actions`) holds `tddy-tools.yaml`, instead of adding
  it to #790's `BUILTIN_TOOLS`: #790's `an_override_with_a_new_id_adds_a_tool` pins the built-in id
  set to `["jq", "rg"]`, so growing that list would break the parent's test. A host that launches
  agents builds its registry with the new **`ToolRegistry::from_embedded(&[…])`** from
  `BUILTIN_TOOLS ++ BUILTIN_SUPPORT_TOOLS ++ builtin_specs()`, then `load_with_owners`.
  `tddy_coding_agent_specs::load_agent_registry(overrides_dir)` is that composition, and what the
  daemon will call.
- **New `ToolError` variants** `UnknownExtension { file, owner }`, `ExtensionRejected { file, owner,
  message }`, `UnboundPlaceholder { tool, name }` — additive, as the Responsibility's refusals need.
- **New `ToolDef` fields** `run: Option<RunSection>` and `extensions: BTreeMap<String, Value>`, both
  `#[serde(default)]`, so #790's files (`rg.yaml`) and fixtures still parse.
- **`HostCodingAgents`** (`spec_launch`) — the registry plus the `ToolHost` it resolves against,
  installed with **`DaemonSessionHost::with_coding_agents`** and carried as
  **`LaunchSessions::coding_agents`**. This is how "claude-cli spawn wiring passes the spec registry
  through" lands; the acceptance suite injects a fixture `ToolHost` through it.
  `HostCodingAgents::resolve(id, binary_override)` takes daemon.yaml `claude_cli.binary_path` as the
  override until #801.
- **Slot input/outputs**: `SlotInputs { workdir, scratch_dir, mcp_server, allowed_tools,
  denied_tools, permission_prompt_tool, preamble, append_system_prompt_file, hooks }` and
  `RenderedSlots { argv_extra, files }`. `argv_extra` order is append-system-prompt, allow, deny,
  permission prompt, MCP — the hand-built order. Allow/deny render **one flag per entry**
  (`--allowedTools a --allowedTools b`), which is what today passes; "one flag per list joined" in
  the plan was read as that.
- **Hooks adapter** is a trait (`slots::hooks::HooksAdapter`) with one adapter,
  `ClaudeSettingsLocal` (`claude-settings-local`), resolved by `hooks::adapter(name)`. The crate
  depends on `tddy-agent-backend` (workspace) for `HookCommandParams` and
  `build_claude_hooks_settings`; no third-party crate was added.

**For green (not decided here):**

- `launch_host_pty` needs a public `CliSessionManager` entry that spawns a rendered argv
  (`spawn_tool` is `pub(crate)` in `tddy-cli-sessions`) — a small additive change there.
- The `tddy-tools` the hooks and MCP slot name: today `resolve_tddy_tools_path` prefers daemon.yaml
  `claude_cli.tddy_tools_path`, then `CARGO_BIN_EXE_tddy-tools`, then the daemon's sibling; the
  registry resolves `TDDY_TOOLS_BINARY`, then `PATH`. The acceptance suite sets both to one stub, so
  either rule passes it; green must pick one without breaking the sibling lookup `./install` relies
  on.
- The start/resume routing is not wired: `TODO(claude-host)` at the spawn in `claude_cli_spawn.rs`
  and at `resume_with_options` in `svc_resume_claude_cli_session.rs`. Nothing installs coding agents
  in production yet (the daemon registration is green's `tddy-daemon` change), so every existing
  path is unchanged.
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
