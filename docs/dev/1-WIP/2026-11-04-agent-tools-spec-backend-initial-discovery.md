# Initial discovery: `SpecBackend` replaces `AnyBackend` (#agent-tools 16/19)

**Created:** 2026-11-04
**Changeset:** [2026-11-04-agent-tools-spec-backend.md](./2026-11-04-agent-tools-spec-backend.md)

Read-only discovery at base `0dbba9195`. Exploration 1 covered the workflow backend and the daemon.yaml config removal together and is copied verbatim. It was split:

| Discovery section | This node (n16) | `feature/agent-tools/headless-driver` (#821) | `feature/agent-tools/agent-config-removal` (n19) |
|---|---|---|---|
| §1.2 backend selection; §2 seam 7; §3 `SpecBackend`/`build_spec_backend`; C1, C2; §5 `AcpDriver::open_session` deadline; model-registry reserved names | ✅ | the headless half of `SpecTransport` (the generic driver) | — |
| §1.3–§1.7, seams 1–6, C3–C6 (readers, answer wait, claude.yaml, fixtures, Telegram delete) | — | — | ✅ |

Exploration 1 recommended keeping the bespoke headless backends behind adapters; the developer chose a generic headless driver instead (#821), so `SpecBackend::Headless` wraps that driver.

## Exploration 1

### n15 agent-config-removal — discovery (read-only, base `0dbba9195`)

Scope: a generic spec-plus-adapter workflow backend so `tddy-coder --agent <any registry id>` builds one; delete `AnyBackend`,
`BUILTIN_BACKEND_AGENT_IDS`, `create_backend`; delete daemon-kernel `claude_cli`/`cursor_cli`/`cli_harnesses` fields, types and readers;
wire `coding_agent.answer_wait_secs` to its callers; declare the fullscreen env and persistent-home key in `claude.yaml`; migrate the
`daemon_config_from_fixture_file` fixtures and delete it; restore worktree removal on a Telegram-initiated delete.
Sources: todo `docs/dev/todo/2026-11-04-removed-agent-config-readers-remain.md`; n12 changeset § Removed + "Deletions green needs
approval for"; `2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md` (same fixture sweep for daemon-rpc).

#### 1. State A

##### 1.1 Crates and edges (relevant)

| Crate | Edge | Note |
|---|---|---|
| `tddy-coder` | → `tddy-core` (re-exports agent-backend), `tddy-coding-agent-specs`, `tddy-actions` | owns `create_backend`, `BUILTIN_BACKEND_AGENT_IDS` |
| `tddy-model-registry` | → **`tddy-coder`** (`Cargo.toml:32`) | reads `tddy_coder::run::BUILTIN_BACKEND_AGENT_IDS` at `src/store.rs:672` (reserved assistant names) |
| `tddy-daemon-kernel` | → core, discovery, sandbox, rpc, daemon-extensions, service, task, github | **no** specs edge — the kernel cannot read a spec; readers must move up to agent-launch/session-lifecycle |
| `tddy-telegram-control` | → core, daemon-kernel, daemon-extensions, host-service, service, session-activity, spawn, rpc, telegram, session-lifecycle | no specs edge directly; gets them through session-lifecycle |
| `tddy-session-activity` | → specs | `delete_session_directory(…, agents: Option<&CodingAgentRegistry>)` |

##### 1.2 Workflow backend selection (tddy-coder + agent-backend)

| Symbol | Location |
|---|---|
| `enum AnyBackend {Claude, ClaudeAcp, Cursor, Codex, CodexAcp, Stub, OpenCode}` + 5 delegating matches (`invoke`, `name`, `submit_channel`, `action_invoke_cache_eligible`, `list_models`) | `tddy-agent-backend/src/backend/mod.rs:42-52, 106-170` |
| `SharedBackend::from_any` / `from_arc` | `backend/mod.rs:57-104` (`from_arc` already exists — the replacement) |
| `AnyBackend::with_provider_route` (Cursor/Stub refusal) | `backend/routed.rs:17-38` |
| `build_backend(agent, paths)` (model catalog's own string match) | `backend/model_catalog.rs:146-170` |
| `create_backend(agent, cursor_path, codex_path, codex_acp_path, opencode_path, _, _, tools_dir, tool_host, defs, route)` | `tddy-coder/src/run.rs:2516-2632` (match :2551-2631, unknown → error listing `BUILTIN_BACKEND_AGENT_IDS` :2614-2625) |
| `BUILTIN_BACKEND_AGENT_IDS` (7 ids) | `run.rs:2637-2645` |
| `create_backend_for_args` | `run.rs:288`; callers `run.rs:1394, 1610, 1799, 3478, 3852` |
| in-file tests | `run.rs:2648+ create_backend_specialized_agent_tests`, `:2867+`, calls at :2730, :2783, :2835, :3023, :3095, :3447 |
| per-agent spec builders | `tddy-coder/src/headless_from_spec.rs:100 cursor_backend_from_spec`, `:111 codex_backend_from_spec`; `claude_from_spec.rs:25`; `acp_from_spec.rs:72,88,107` (claude-acp / codex-acp / opencode) |
| registry listing (n12) | `tddy-coder/src/agent_registry_cli.rs:18 backend_agent_ids` (headless ∨ acp section, + `stub`), `:30 backend_selection_question_from` |
| `apply_provider_route(AnyBackend, …)` | `tddy-coder/src/provider_route_arg.rs:38-48` |
| oz | has `headless` (`oz.yaml`), listed by `backend_agent_ids`, **not** buildable by `create_backend` → the n12 AC "`--agent` accepts any registry id" is open |

Backend building blocks already generic: ACP — `ClaudeAcpBackend`/`CodexAcpBackend`/`OpenCodeAcpBackend` are each `AcpAgentCommand` + optional
`AcpHooksAdapter` + deps over the shared `AcpDriver` (`backend/acp.rs`, `codex_acp.rs:110 with_launch`, `opencode_acp.rs`). Headless —
`HeadlessDriver::new(HeadlessHarness, HeadlessCommand)` (`backend/headless_driver.rs:129-155`) keyed by the stream adapter's
`turn_format()` (`adapters/mod.rs:109`); `ClaudeCodeBackend`/`CursorBackend`/`CodexBackend` (859/822/843 lines) take a
`HeadlessRenderer` (`adapters/headless.rs:96`) but keep per-agent MCP/tool-executor code (e.g. `cursor.rs:316` writes `.cursor/mcp.json`).
`HeadlessHarness {Claude, Cursor, Codex, Oz}` (`headless_driver.rs:46-77`) remains a closed enumeration.

`AnyBackend` refs outside `backend/mod.rs`: 21 files (`rg -c`), most `AnyBackend::Stub(StubBackend::new())` in tests: `tddy-e2e/src/lib.rs` ×5,
`tddy-e2e/examples/rpc_demo.rs`, `tddy-livekit/tests/acp_session_livekit.rs`, `tddy-integration-tests/tests/{acceptance_tdd_graph_engine,tool_session_provider_route}.rs`,
`tddy-tui-testkit/tests/virtual_tui.rs`, `tddy-service/src/integration_tests.rs` ×6, `tddy-presenter/src/{lib.rs:18, presenter/presenter_impl.rs:1083-1098}`.

##### 1.3 daemon.yaml agent sections (tddy-daemon-kernel)

| Symbol | Location |
|---|---|
| `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}` | `src/config.rs:155, 158, 162`; `Default` :246-248 |
| re-exports | `config.rs:313-318` |
| refusal (wired) | `config/daemon_methods.rs:39` → `refuse_removed_agent_keys` (`config/removed_agent_keys.rs:70`) |
| `ClaudeCliConfig` {binary_path, tddy_tools_path, daemon_url, claude_home_dir, fullscreen, answer_wait_secs} | `config/agent_cli.rs:148-180` |
| `CliHarnessConfig` | `agent_cli.rs:184-188` |
| `CursorCliConfig` | `agent_cli.rs:248-262` |
| readers | `resolve_claude_binary_path` :114, `claude_cli_answer_wait_secs` :194, `claude_cli_renderer_env` :205, `cli_harness_binary` :220, `resolve_cursor_binary_path` :337, `resolve_cursor_cli_tddy_tools_path` :354, `resolve_cursor_cli_daemon_url` :368, `resolve_cursor_home_dir` :385, `resolve_claude_home_dir` :414 (no caller outside kernel); `daemon_hook_urls.rs:37 claude_hook_daemon_url` |
| keepers (not config) | `find_real_harness_on_host` :294, `resolve_harness_binary_name` :133, `CLAUDE_FULLSCREEN_ENV = "CLAUDE_CODE_NO_FLICKER"` :32, `choose_*_binary`, `find_real_harness_in` |
| kernel tests | `config.rs:993 claude_cli_config_tests`, `:1218 cursor_cli_config_tests`; `tests/answer_wait_secs_config.rs` (4 asserts on `claude_cli`) |

Readers outside the kernel (production, 28 sites in 19 files):

| File:line | Reads |
|---|---|
| tddy-agent-launch/src/chat.rs:21,38,59 | cursor tddy-tools path, answer wait, cursor daemon url |
| …/cursor_cli_spawn.rs:24,407,515; cursor_from_spec.rs:15,23 | cursor binary, answer wait |
| …/svc_start_sandboxed_cursor_cli_session.rs:268,539 | tddy-tools path, answer wait |
| …/claude_cli_spawn_steps.rs:188,194,198 | `.claude_cli` tddy_tools_path, hook url, answer wait |
| …/claude_cli_spawn.rs:279; svc_resume_claude_cli_session.rs:108,305; svc_start_sandboxed_claude_cli_session.rs:284,394; svc_relaunch_sandboxed_runner.rs:129,158 | `.claude_cli` + `claude_cli_renderer_env` |
| …/spec_launch.rs:475-482 `configured_binary` | `claude_cli.binary_path` |
| …/host_coding_agents.rs:45-50 (`DaemonToolHost::located`) | `resolve_claude_binary_path`, `claude_cli.tddy_tools_path` |
| …/hooks_and_urls.rs:147; notify_env.rs:99 | claude binary, hook url |
| …/harness_from_spec.rs:209-226 `configured_cli_binary` (+`TODO(agent-registry)`); opencode_cli_spawn.rs:100 (dead, `TODO(harness-clis)`); cli_harness_jail.rs:285 | `cli_harness_binary`, cursor tddy-tools |
| tddy-session-lifecycle/src/coding_agent_launcher.rs:70,986,987 (`headless_program` matches `HeadlessHarness`) | claude/cursor binary |
| …/connection_service/svc_activity_ports.rs:103 | `AnswerWaitPorts.wait` (daemon-wide) |
| …/connection_service/svc_terminal_ports.rs:268 | agent terminal tabs via legacy `CliHarness` + `cli_harness_binary` |
| tddy-session-split/src/svc_spawn_split_agent.rs:251,257; service_util.rs:328 | hook url, answer wait, claude binary |
| tddy-daemon-rpc/src/catalog/ports.rs:111-129 | `agent == "cursor"` gate + `resolve_cursor_binary_path` + `cli_harness_binary` (model listing) |
| tddy-telegram-control/src/telegram_session_control/cli_spawns.rs:118-124, 357 | `claude_cli.binary_path`, cursor binary (Telegram's hand-built launcher) |

Totals: `rg` of field/type/reader names → **112 lines in 39 files** (kernel 45 in 2 files).

##### 1.4 `answer_wait_secs`
Spec field exists: `CodingAgentSection.answer_wait_secs` (`tddy-coding-agent-specs/src/spec.rs:196-199`), `CodingAgentSpec::answer_wait_secs()` :208,
`DEFAULT_ANSWER_WAIT_SECS = 120` :204 (duplicated in kernel `agent_cli.rs`). claude.yaml does not set it. Callers still on the kernel:
`claude_cli_spawn_steps.rs:198`, `cursor_cli_spawn.rs:407`, `chat.rs:38`, `svc_start_sandboxed_cursor_cli_session.rs:539`,
`svc_spawn_split_agent.rs:257` (hook params → `claude_hooks.rs:51,118-120` writes hook `timeout = wait + margin`), and the server-side
hold `svc_activity_ports.rs:103` → `tddy-session-activity/src/service/question_answers.rs:47` (`AnswerWaitPorts.wait`, one `Duration` per daemon).

##### 1.5 claude.yaml gaps
`home: { kind: persistent, key: claude }` **already declared** (`claude.yaml:93`; cursor.yaml:91). The *daemon.yaml* knob it replaces is
`claude_cli.claude_home_dir` (absolute dir) → refusal says `home.key` (`removed_agent_keys.rs:130`), but `HomeKind::Persistent{key}`
(`tddy-actions/src/tool/home.rs:20`) is a key under `<tddyhome>/homes/`, not a path — an operator with a custom dir has no equivalent (decide).
Fullscreen: `run.env: {}` (`claude.yaml:29`); `claude_cli_renderer_env` adds `CLAUDE_CODE_NO_FLICKER=1` by default → declare
`run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`. `plan_host_pty` already copies `run.env` (`spec_launch.rs:248-254`); the jail/relaunch paths add the
kernel env separately (`svc_start_sandboxed_claude_cli_session.rs:394`, `svc_relaunch_sandboxed_runner.rs:158`) — check they read `run.env` or they lose it.

##### 1.6 Fixtures
`tddy_session_lifecycle::test_util::daemon_config_from_fixture_file` (`src/test_util.rs:58-70`, `TODO(agent-registry)` :64) = `serde_yaml::from_str`
bypassing the refusal. **40 caller files** (~42 calls): tddy-session-lifecycle 34 (32 `tests/*`, `tests/*_support/mod.rs` ×6 among them,
`src/connection_service/stack_child_spawn_tests.rs`), tddy-daemon 4 (`split_session_resume_acceptance`, `session_room_cross_host_acceptance`,
`session_agent_remote_acceptance`, `remote_managed_worktree_cross_host_acceptance`), tddy-daemon-rpc 2 (`session_room_exec_tool_acceptance`,
`project_home_session_tools_acceptance`). Keys they set under the removed sections: `binary_path` ~26, `tddy_tools_path` 4, `claude_home_dir` 1;
`cli_harnesses.{session_type}` in `cli_harness_support/mod.rs:52`, `sandboxed_cli_harness_acceptance.rs:341`. Struct literals: `ClaudeCliConfig`
in `tddy-telegram-control/tests/{telegram_branch_conflict_acceptance.rs:109, telegram_start_claude_acceptance.rs:90}`, `CursorCliConfig` in
`telegram_start_cursor_acceptance.rs:78`; `.claude_cli` reads in `claude_cli_fullscreen_renderer.rs:176`, `unified_worktree_base_acceptance.rs:73`.
Target shape exists: `tests/agent_registry_support/mod.rs:203 a_daemon_with_tools`, `:228 with_coding_agents(HostCodingAgents::new(registry, fixture host))`
— the stub is reached through the fixture `ToolHost`'s PATH/env (per host, not process env). `test_util::builtin_coding_agents()` :79.

##### 1.7 Telegram delete
`tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs:412-420` passes `None` for `agents` + `TODO(agent-registry)` naming
`docs/dev/todo/2026-11-04-telegram-delete-leaves-agent-worktrees.md` — **that todo file does not exist** (dangling reference). Effect:
`worktree_removal_applies_to(None, st)` (`tddy-session-activity/src/session_deletion.rs:43-53`) is `false` for every agent session type → worktree
left behind. Controller built in `tddy-daemon/src/runtime/telegram_wiring.rs:172` (`TelegramSessionControlHarness::with_workflow_spawn_and_telegram_tracked`);
host coding agents loaded in `runtime/mount_host.rs:263`. Other callers of `delete_session_directory` pass a registry or `None` by design
(`svc_signal_delete_session.rs:140`, `workspace_branch_start.rs:113`, `svc_start_sandboxed_codebase_session.rs:148`; tests at session_deletion.rs:545-745).

#### 2. Seams and order

1. **Spec-side first (additive):** `claude.yaml` `run.env: {CLAUDE_CODE_NO_FLICKER: "1"}`, `answer_wait_secs: 120` (optional, default equal).
2. **Answer wait:** hook-param callers read `agent.spec.answer_wait_secs()` (they hold the spec); `AnswerWaitPorts.wait: Duration` →
   `wait_for: Arc<dyn Fn(&session_type) -> Duration>` (or per-call lookup via registry + session metadata) so the server hold equals the hook's.
3. **Readers → host/spec:** binary = `HostCodingAgents::resolve(id, None)` (env var via spec `binary.env`, host auto-resolution moves into
   `DaemonToolHost::located` without the config); tddy-tools = `tools/tddy-tools.yaml` / host; hook url = `local_daemon_hook_url`/`advertise_url`
   (`daemon_hook_urls.rs:20-33`); catalog `ports.rs` probe via spec. Each reader edit compiles with the fields still present.
4. **Fixtures:** replace `daemon_config_from_fixture_file` + section with `test_config()`/`DaemonConfig::load` + a fixture host mapping
   `TDDY_CLAUDE_BINARY`/`TDDY_CURSOR_AGENT`/`TDDY_OPENCODE_CLI`… → stub; add `test_util::coding_agents_with_binaries(&[(id, path)])`.
   Telegram struct literals drop the field. Then delete `daemon_config_from_fixture_file`.
5. **Delete** the three fields, three types, the readers, kernel tests, `answer_wait_secs_config.rs`; keep `refuse_removed_agent_keys`.
6. **Telegram:** `TelegramSessionControlHarness::with_coding_agents(Arc<CodingAgentRegistry>)` (or `HostCodingAgents`); wire in
   `telegram_wiring.rs`; pass to `delete_session_directory`. Also replaces `cli_spawns.rs:118-124,357` binary reads (unless n14 removed that launcher).
7. **Backend:** add `SpecBackend` (below) and `build_spec_backend`; `create_backend` → defs, then `stub`, then registry → spec backend, else
   refuse naming `backend_agent_ids`. Swap `apply_provider_route` to the trait object. Replace `AnyBackend::Stub` test uses with
   `SharedBackend::from_arc(Arc::new(StubBackend::new()))`. `tddy-model-registry` reserved names ← registry ids (it already has
   `reserved_agent_ids`, store.rs:677 — feed it). Then delete `AnyBackend`, `routed.rs` enum impl, `BUILTIN_BACKEND_AGENT_IDS`, `build_backend` (model_catalog).

Steps 1-6 (config) and 7 (backend) are independent seams — two PRs are possible.

#### 3. Proposed owned API surface

```rust
// tddy-agent-backend
pub struct SpecBackend { /* id, transport: SpecTransport, route: Option<HarnessRoute>, progress */ }
pub enum SpecTransport {
    Headless { renderer: Arc<dyn HeadlessRenderer>, adapter: Arc<dyn StreamAdapter> },   // HeadlessDriver(adapter.turn_format()?)
    Acp { command: AcpAgentCommand, hooks: Option<Arc<dyn AcpHooksAdapter>>, deps: BTreeMap<String, PathBuf> },
}
impl CodingBackend for SpecBackend { … }
impl SpecBackend { pub fn with_provider_route(self, route: HarnessRoute) -> Result<Self, RouteRefusal>; } // refused when spec has no route

// tddy-coder
pub fn build_spec_backend(registry: &CodingAgentRegistry, id: &str, host: Arc<dyn ToolHost>) -> anyhow::Result<SharedBackend>;
// headless preferred? acp preferred? — the ACP-over-headless preference is `headless_driver.rs:6-7` ("ACP is preferred wherever a harness offers both")

// tddy-session-activity
pub struct AnswerWaitPorts { pub pending, pub presence, pub wait_for: Arc<dyn Fn(Option<&str>) -> Duration + Send + Sync> }
// tddy-telegram-control
impl<S> TelegramSessionControlHarness<S> { pub fn with_coding_agents(self, agents: Arc<CodingAgentRegistry>) -> Self; }
// tddy-session-lifecycle::test_util
pub fn coding_agents_with_binaries(binaries: &[(&str, &Path)]) -> Arc<HostCodingAgents>;
```

Decision for the developer: keep the three bespoke headless backends behind spec-named backend adapters (e.g. `coding_agent.headless.backend:
claude-code`) so byte-for-byte behaviour (MCP file, tool executor) is preserved, with `SpecBackend` as the default for agents naming none — or
fold them into `SpecBackend` (much larger; `oversized-file-claude/cursor/codex.md`). Recommend the former for n15.

**Removed:** `AnyBackend` (+5 matches, `routed.rs` impl, `SharedBackend::from_any`), `build_backend` (model_catalog.rs:146), `create_backend`'s match
and `BUILTIN_BACKEND_AGENT_IDS`, `DaemonConfig.{claude_cli,cursor_cli,cli_harnesses}`, `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig`,
the 10 readers in §1.3, `daemon_hook_urls::claude_hook_daemon_url`, `configured_binary`, `configured_cli_binary` (+TODO), `configured_cursor_binary`,
`test_util::daemon_config_from_fixture_file`, kernel `DEFAULT_ANSWER_WAIT_SECS` (keep specs'), `tests/answer_wait_secs_config.rs`.
All need deletion approval (CLAUDE.md § ASK; n12 changeset already lists them).

#### 4. Candidate acceptance tests

| # | Test | File | Fixture |
|---|---|---|---|
| C1 | `tddy_coder_agent_builds_a_backend_for_an_agent_declared_only_in_tddyhome_tools_and_runs_a_turn_through_its_adapter` (acme, `claude-stream-json`, stub binary prints a stream) | `packages/tddy-coder/tests/agent_registry_cli_acceptance.rs` (extend; `ACME_YAML` :14) | own `<tddyhome>/tools` + stub |
| C2 | `tddy_coder_agent_oz_builds_its_headless_backend` / `an_unknown_agent_is_refused_listing_the_registry_ids` | same | builtin registry |
| C3 | `an_override_of_claude_yaml_with_answer_wait_secs_holds_a_permission_hook_that_long_and_writes_that_timeout_into_the_hooks_file` | `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs` | `agent_registry_support`, `a_hook_reports_running` :330 |
| C4 | `a_claude_cli_start_has_claude_code_no_flicker_from_claude_yaml_and_an_override_without_it_does_not` | `claude_cli_fullscreen_renderer.rs` (rewrite off `.claude_cli` :176) | `builtin_coding_agents`/override tools dir |
| C5 | `a_telegram_delete_of_a_claude_cli_session_removes_its_worktree_and_of_a_cursor_cli_session_keeps_it` | `packages/tddy-telegram-control/tests/telegram_session_control_integration.rs` | telegram harness + registry |
| C6 | `the_session_lifecycle_fixtures_start_every_agent_from_a_daemon_yaml_with_no_agent_section` (source scan: no `daemon_config_from_fixture_file`, no `claude_cli:` in test sources) | `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs` (sibling scan) | — |

Existing to keep green: `tddy-daemon/tests/removed_agent_config_acceptance.rs` (refusals), `tddy-daemon-kernel/tests/removed_agent_keys_unit.rs`.
Unit: `SpecBackend` picks ACP over headless when both exist; refuses a route for a spec with no `route`; `with_provider_route` on stub refused;
`AnswerWaitPorts::wait_for` returns the spec value per session type and the default for an unknown one; `DaemonToolHost::located` finds `claude`
without any config; model-registry refuses an assistant named like any registry agent (`model_registry_reserved_names_unit.rs`).

#### 5. Risks

- **Hangs:** `AnswerWaitPorts` hold must stay bounded and ≤ the hook timeout the agent was launched with (`claude_hooks.rs:118-120`): reading the
  wait per session at hold time while the hook file was written at start from a since-edited override could exceed Claude's `timeout` → the agent
  kills the hook (not a hang) — record the wait in session metadata at start instead of re-reading. A generic `SpecBackend::Acp` reaches
  `AcpDriver::open_session` with no deadline (`2026-11-04-acp-launch-waits-have-no-deadline.md`) — every dropped-in ACP agent inherits it.
- **Fallback creep:** `resolve_claude_binary_path`'s step 4 "configured value as-is" and `cli_harness_binary`'s `default_binary` are fallbacks; the host
  resolver replacing them must refuse a missing binary naming `binary.env`. `create_backend` must not fall through to claude for an unknown id
  (it refuses today, `run.rs:2614-2625`). Telegram with no registry must refuse the delete's worktree decision loudly or be given one — never `None`.
  `AnswerWaitPorts::default()` (no orchestrator) stays the no-op.
- **Breaking config:** operators lose `claude_home_dir` (absolute) with no spec equivalent (`home.key` is a name under `<tddyhome>/homes`); `daemon_url`
  → `listen.advertise_url` changes the cursor hook url too (it fell back cursor → claude → port, `agent_cli.rs:354-380`). Changelog entry.
- **Linux/macOS:** persistent home mount differs per jail backend (`tddy-sandbox-recipes::homes::persistent_home`, `jail_launch.rs:286-294`); the
  fullscreen env must reach the jailed process (`svc_start_sandboxed_claude_cli_session.rs:394` adds it outside `run.env` today).
- **Proto/web:** none. `config.example.yaml`, `dev.daemon.yaml:130` (comment), `desktop.yaml.production` already section-free (only `dev.daemon.yaml` mentions it).
- **Docs at wrap:** `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `{opencode,muse,warp}-cli-session.md` (binary config), `session-agent-roster.md`,
  `telegram-session-control.md` (delete), `docs/ft/coder/1-OVERVIEW.md` (`--agent`), `packages/tddy-daemon-kernel` README/docs, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md`
  (`run.env`, `answer_wait_secs`), changelog `docs/ft/daemon/changelog/` breaking entry. Create or drop the dangling todo `2026-11-04-telegram-delete-leaves-agent-worktrees.md`;
  close `2026-11-04-removed-agent-config-readers-remain.md`, `2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md` if the daemon-rpc fixtures move too.

#### 6. Overlap

| With | Edge |
|---|---|
| n14 | n14 deletes ~14 of the 28 outside readers (all claude/cursor launch-path sites in §1.3) and possibly Telegram's `cli_spawns.rs` launcher. n15 before n14 edits code n14 deletes, and both rewrite the same 20 session-lifecycle suites (fixtures vs launch). **n14 before n15.** |
| n13 | `routed.rs` (`AnyBackend::with_provider_route` names `Harness::Cursor`) — n13 edits, n15 deletes; `SpecBackend::with_provider_route` consumes n13's agent-keyed `RouteRefusal`. **n13 before n15.** |
| backend half | depends on neither n13's launch half nor n14; only on n13's `RouteRefusal` shape. Could be split out as its own node and run in parallel with n14. |
| `coding_agents: Option` | fixture sweep here is where `LaunchSessions.coding_agents` can become non-optional (todo 2026-11-02) — natural to do in n15. |

#### 7. Complexity

**5** — row-4 signals several times over: ≥8 crates (coder, agent-backend, daemon-kernel, agent-launch, session-lifecycle, session-activity,
session-split, daemon-rpc, telegram-control, model-registry, e2e/integration test crates); a breaking config-type removal (data/contract);
a new backend type over a spawned-process seam; ~40 fixture files. **Recommend splitting**: n15a backend (`SpecBackend`, `AnyBackend`/`BUILTIN_BACKEND_AGENT_IDS`/
`create_backend` removal, model-registry), n15b config removal (readers, answer wait, claude.yaml, fixtures, Telegram).

#### 8. Code issues touched

| Record | Metric | Claimed |
|---|---|---|
| `tddy-agent-backend/docs/code-issues/oversized-file-backend-mod.md` | 850 prod lines; `AnyBackend` removal shrinks it | unclaimed |
| `tddy-coder/docs/code-issues/oversized-file-run.md` | 2,707 prod lines; `create_backend` ≈120 | unclaimed |
| `tddy-coder/docs/code-issues/complexity-run-run-with-args.md` | 195 lines | unclaimed |
| `tddy-agent-backend/docs/code-issues/oversized-file-{claude,cursor,codex}.md` | only if bespoke backends fold | unclaimed |
| `tddy-session-lifecycle/docs/code-issues/oversized-file-coding-agent-launcher.md` | 685 (`headless_program`) | unclaimed |
| `tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-chaining-and-listing.md` | 518, +delete wiring | unclaimed |
| `tddy-daemon-kernel/docs/code-issues/*` (privilege-drop) | not touched | — |

None `Claimed by:`.
