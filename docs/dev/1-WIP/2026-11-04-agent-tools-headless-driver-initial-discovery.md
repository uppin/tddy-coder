# Initial discovery: one spec-configured headless driver (#agent-tools 15/19)

**Created:** 2026-11-04
**Changeset:** [2026-11-04-agent-tools-headless-driver.md](./2026-11-04-agent-tools-headless-driver.md)

Read-only discovery at base `0dbba9195`. Exploration 1 is the backend-plus-config discovery, copied verbatim; only its §1.2, the backend half of §2–§4 (seam 7, `SpecBackend`, C1/C2) and the backend rows of §5–§8 bear on the headless backends. Its backend half is split between this node and `feature/agent-tools/spec-backend` (n16); its config half is `feature/agent-tools/agent-config-removal` (n19). It recommended *keeping* the bespoke headless backends behind spec-named adapters; the developer decided instead: **"make headless generic too"** — hence this node, and Exploration 2, which is this node's own discovery.

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

## Exploration 2 — the bespoke headless workflow backends against `HeadlessDriver` and the spec

Read-only, base `0dbba9195`. Paths are repo-relative.

#### Framing fact

`HeadlessDriver` (`packages/tddy-agent-backend/src/backend/headless_driver.rs`, 563 lines) is **not** a workflow backend. It implements `ConversationDriver` (returns `AcpTurnEnd`/`AcpUpdate`, async tokio) and powers headless coding *subagents*. The three bespoke backends implement `CodingBackend::invoke(InvokeRequest) -> InvokeResponse` with a blocking `std::process` inside `spawn_blocking`. Both families already render argv from the same `SpecHeadlessRenderer`/`HeadlessTurn` port; they differ in request mapping, environment, logs, errors and the response.

#### 1. The bespoke backends

| | Claude | Cursor | Codex |
|---|---|---|---|
| File (lines) | `backend/claude.rs` (859) + `claude/transcript_usage.rs` (145) | `backend/cursor.rs` (822) | `backend/codex.rs` (843) + `codex/login_text.rs` (48) |
| Type | `ClaudeCodeBackend` :282 | `CursorBackend` :56 | `CodexBackend` :48 |
| Constructors | `new` :334, `with_path` :345, `with_progress` :356, `with_headless_renderer` :368, `with_provider_route` (`routed.rs:41`) | `new` :107 (`DEFAULT_CLI_BINARY="agent"` :104), `with_path` :117, `with_progress` :127, `with_headless_renderer` :138; no route | `new` :72, `with_path` :81, `with_headless_renderer` :92, `with_provider_route`; no `with_progress` |
| Dispatch | `invoke_sync` :454 → `invoke_from_spec` :380 if renderer, else hand-built | `invoke_sync` :338 → `invoke_from_spec` :150 | `invoke_sync` :246 → `invoke_from_spec` :106 |
| Hand-built argv | `build_claude_args` :77 (pub) | `build_cursor_cli_args` :285 | `build_codex_exec_argv` :548 |
| Goal-hint params | `permission_mode` (`plan` only on `agent_cli_plan_mode`; `acceptEdits`) :200-221,:258; `allowed_tools`; `permission_prompt_tool = mcp__tddy-tools__approval_prompt` when a tool server exists :414; model dropped if route args contain `--model` :401 | prompt merged with system prompt :164; `force=1` always, `plan=1` on plan mode :159-162 | `sandbox_mode` read-only iff plan mode and ReadOnly :122; model `None` whenever routed :117 |
| System prompt | `--append-system-prompt-file`; inline → temp file with cleanup guard :234-255 | prepended to prompt (`system_content` :34) | prepended (`merge_codex_prompt` :509) |
| Session | fresh `--session-id`, resume `--resume` | resume `--resume` | `resume … <id>`; the workflow engine swaps in `codex_thread_id` by `name()` (`tddy-workflow-engine/src/workflow/backend_invoke_task.rs:121,253`) |
| MCP file | legacy `$TMPDIR/tddy-mcp-*.json` (silently skipped when `tddy_tools_path()` fails, :465-472); spec `<session_dir>/claude-mcp-config.json` 0600, never deleted | `.cursor/mcp.json` in the workdir (`register_cursor_mcp_config` :316 / spec slot :174), never removed or git-excluded | none (`mcp_server: None` :140) |
| Stream | `ClaudeStreamJson` → `process_ndjson_stream` | `CursorStream` → `process_cursor_stream` | `CodexJsonl` → `parse_codex_jsonl_output` |
| `exit_ok_if_structured` | spec `claude.yaml` true | unset (false) | spec `codex.yaml` true |
| Progress | `progress_sink` else instance callback | same | no-op :413-414 |
| Env | PATH, route env, `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, `remote.env_pairs()`, spec env | same, no route | PATH, route env, `BROWSER=current_exe` + `TDDY_CODEX_OAUTH_OUT` :310-322, `TDDY_*`; no `remote.env_pairs()` |
| Error text | stream errors, else stderr | stderr + `Invoked:` | last stream error, `codex_stderr_brief_for_user`, `codex_openai_auth_remediation` :470 |
| `list_models` | curated default (`mod.rs:840`) | `agent --list-models` on `self.binary_path` :218 — the spec path builds `CursorBackend::new()` so it ignores the spec-resolved binary (`tddy-coder/src/headless_from_spec.rs:105`); `.output()` has no deadline | curated default |
| `submit_channel` / `action_invoke_cache_eligible` | trait defaults (`mod.rs:824-836`) | same | same |

All three set `current_dir(request.working_dir)`. Cancellation is the global `CHILD_PID` (`mod.rs:172-198`): one slot, SIGKILL of one pid, no process group — two concurrent backends overwrite each other.

**Waits with no deadline (all three):** `adapter.read_stream` to EOF; `stderr_thread.join()` (claude.rs:707, cursor.rs:557, codex.rs:420) — hangs if a grandchild inherits stderr; `child.wait()` (:711/:561/:424). The three `run_child` bodies (claude :495-782, cursor :368-632, codex :268-505) are ~90% identical.

#### 2. `HeadlessDriver` and the spec

Generic already: one turn at a time (`begin_turn` :170); session id from the transcript (:349); async spawn with `process_group(0)`, `kill_on_drop`, stdin null (:194-230); `killpg` on cancel / slot drop / driver drop (:394-440); last 20 stderr lines with `STDERR_GRACE` 2s (:40-43,:469); `run_rendered_turn` :325-381. Delegates argv to `HeadlessRenderer::render` (no renderer → `argv::turn_argv` :248) and stdout to `Transcript::new(HeadlessHarness, …)` chosen by `adapter.turn_format()` (`adapters/mod.rs:109`) — a closed enum (`transcript.rs:51-58`). **No turn deadline**; after EOF `child.wait()` is bounded only by cancel (:294-300). `HeadlessHarness {Claude, Cursor, Codex, Oz}` :46-77.

Spec `HeadlessSection` (`tddy-coding-agent-specs/src/spec.rs:533-551`): `argv`, `env` (literal), `adapter`, `exit_ok_if_structured`, `mcp`. The renderer (`src/headless.rs:150-218`) binds `prompt`, `model`, `permission_mode`, `workdir`, `session_id`, `resume`, `params.*`, slots and `route_args`.

Per-agent differences **not** expressible as spec fields today:
1. goal-hint → params (claude `permission_mode`, cursor `plan`/`force`, codex `sandbox_mode`);
2. system-prompt delivery (flag vs prepend);
3. whether to bind the approval tool;
4. whether a route replaces the model (codex always; claude only when route args carry `--model`);
5. whether the workflow registers the MCP server (codex does not);
6. who mints a fresh session id (claude: caller; cursor/codex: agent) and the engine's `name()` check for codex;
7. dynamic env values (`BROWSER` = self exe, `TDDY_CODEX_OAUTH_OUT` = `{session_dir}/…`);
8. error enrichment; 9. the response `stderr` rule; 10. progress reporting; 11. route allowed (cursor refuses, `routed.rs:27`).

#### 3. Callers

Production: `tddy-coder/src/run.rs` `create_backend` :2516-2632 (cursor :2552-2562 → `headless_from_spec::cursor_backend_from_spec` :100; codex :2568-2577 → `codex_backend_from_spec` :111; claude :2610-2614 → `claude_from_spec::claude_backend_from_spec` :25, which duplicates `renderer_for` :73); `run.rs:1325` `CodexBackend::with_path` for `spawn_oauth_login`; `backend/model_catalog.rs:148-157` `build_backend` (renderer-less, for `list_models`); `routed.rs:22-30`; `codex_acp.rs:33`, `codex_acp/oauth.rs:21`; `AnyBackend` `mod.rs:42-52`, arms :109-163, `preselected_index_for_agent` :790. `HeadlessDriver::new`: `tddy-coding-subagent/src/session.rs:126` (from `tddy-session-agents/src/coding_agent.rs:166-178`), `tddy-session-lifecycle/src/coding_agent_launcher.rs` `headless_plan` :752, `headless_plan_from_spec` :327-376 (`Claude.id()` :355, `Codex.id()` :359), hand-built match :805-846, `headless_program` :984-991. Adapters returning `HeadlessHarness`: `claude_stream_json.rs:46`, `cursor_stream.rs:50`, `codex_jsonl.rs:71`, `oz_ndjson.rs:69`.

Tests: `tddy-integration-tests/tests/{backend_args (16), cursor_backend (11), codex_backend (8, 1 ignored), conversation_output (5), tool_session_provider_route (4), refactor_integration (5), update_docs_integration (4), validate_refactor_integration (11)}.rs`; `tddy-agent-backend/tests/{headless_driver_acceptance (11), headless_renderer_port (8)}.rs`; `tddy-coder/tests/{claude_headless_from_spec_acceptance (3), headless_cursor_codex_from_spec_acceptance (3)}.rs`; specs `headless_render_golden (6)`, `headless_others_render_golden (13)`, `cursor_spec_acceptance (2)`; session-lifecycle `headless_subagents_from_spec (3)`, `headless_claude_subagent_from_spec (1)`; coding-subagent `headless_session_turns`, `headless_session_permission_yield`; unit tests in claude/cursor/codex.rs (3/9/10), `argv.rs` (5), launcher (24). Fixtures: `#!/bin/sh` stubs echoing `$*` to `argv.log` then `cat`-ing a JSONL stream (`headless_driver_acceptance.rs:26-50`); `tests/fixtures/oz/*.ndjson`; tddy-coder spec tests use a fixture `ToolHost` naming stub binaries.

#### 4. Seams

Spec fields (`coding_agent.headless.*`): `system_prompt: flag | prepend` (or inferred from `instructions.append_system_prompt`); `goal_params: { plan_mode, read_only, accept_edits, always }`; `approval_tool` (or inferred from `permission_prompt`); `workflow_mcp`; `route_replaces_model`; `session.fresh: caller | agent`; templated env values (`{session_dir}`, `{self_exe}`); `scratch: session_dir | workdir`. Existing fields reused: `route` (routable or not), `models.list_command` + `cursor-models`.

Named adapter hooks on `StreamAdapter`: `failure_detail(stream, stderr)` (claude joins errors; codex last error + brief + auth remediation; cursor stderr) and `keeps_stderr`. Replace `turn_format() -> Option<HeadlessHarness>` with a transcript factory on the adapter so `HeadlessHarness` collapses (the note at `headless_driver.rs:113` says the same).

Delete: `build_claude_args` (pub, re-exported `lib.rs:36`), `ClaudeInvokeConfig`/`PermissionMode`, `create_mcp_config_temp_file`, `build_cursor_cli_args`, `register_cursor_mcp_config`, `build_codex_exec_argv`, the three `run_child` copies, `headless_driver/argv.rs` and its `renderer: None` branch, the launcher's `headless_program`, `mcp_servers_wrapped`, `mcp_servers_by_name`, `write_cursor_project_mcp_config`, `OZ_BINARY`, the duplicate `claude_from_spec.rs`. Keep: `spawn_oauth_login`, `CODEX_*_FILENAME`, `write_codex_thread_id_file`, `transcript_usage`, `which_binary`/`tddy_tools_path`, `resolve_codex_binary` (codex-acp, OAuth).

#### 5. Risks

- Hangs: stderr `join()` and `wait()` without a deadline; global single-pid kill. The generic driver should reuse the process group + stderr grace and add a turn deadline.
- Fallbacks: legacy claude silently drops MCP (claude.rs:465); `turn_workdir` falls back to the process cwd; cursor legacy uses `.`; cursor `list_models` ignores the spec binary.
- Byte-for-byte pins: argv goldens, `<session_dir>/claude-mcp-config.json`, codex auth-remediation text, `--permission-prompt-tool`/`--mcp-config` positions (`backend_args.rs`), cursor `--plan`, `--force --trust --approve-mcps`, prompt merge order (`cursor_backend.rs`), the `tddy-request` first line (`conversation_output.rs`). `name()` strings are load-bearing (workflow engine, context globs, curated models, `preselected_index`).
- Behaviour changes to decide: codex gains remote env and progress; the `stderr` rule; cursor `exit_ok`; spec-written MCP files never cleaned up; `.cursor/mcp.json` in the worktree without a git exclude.

#### 6. Markers and records

`TODO(claude-headless)`: claude.rs:290, headless_driver.rs:105, coding_agent_launcher.rs:189. `TODO(headless-others)`: cursor.rs:62, codex.rs:55, headless_driver.rs:116, coding_agent_launcher.rs:32. Code issues (`packages/tddy-agent-backend/docs/code-issues/`): `complexity-claude-run-child.md` (330 lines), `complexity-claude-process-ndjson-stream.md`, `complexity-cursor-process-cursor-stream.md`, `complexity-headless-driver-run-turn.md`, `oversized-file-{claude (767), cursor (634), codex (606), headless-driver (563), backend-mod (850)}.md` — all unclaimed. Todos: `2026-11-02-claude-hand-built-headless-paths.md`, `2026-11-03-headless-others-legacy-argv-deletion.md` (the deletion lists above), `2026-10-25-headless-cursor-codex-oz-cannot-withdraw-native-tools.md` (adjacent, needs real-binary proof; not in scope).
