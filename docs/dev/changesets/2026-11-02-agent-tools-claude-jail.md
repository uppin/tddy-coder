# 2026-11-02 — A sandboxed Claude CLI session is launched from its tool definition

**Type:** Feature

`#agent-tools 5/12` ([PR #794](https://github.com/uppin/tddy-coder/pull/794)), base
`feature/agent-tools/build-tools` ([#793](https://github.com/uppin/tddy-coder/pull/793)). Packages:
`tddy-actions`, `tddy-coding-agent-specs`, `tddy-sandbox-recipes`, `tddy-sandbox-runner`,
`tddy-sandbox`, `tddy-daemon-sandbox`, `tddy-agent-launch`, `tddy-session-lifecycle` (tests),
`tddy-build` (a test literal).

## State A

- `start_sandboxed_claude_cli_session` (342 lines) and `relaunch_sandboxed_runner` each hand-built the
  runner's argv with Claude flags (`--claude-binary`, `--claude-arg`, `--permission-mode`,
  `--dangerously-skip-permissions`, `--resume`, `--append-system-prompt-file`); the runner built
  Claude's argv (`build_claude_base_argv`) and wrote `$TMPDIR/claude-mcp-config.json`.
- `SandboxRecipe::ClaudeCli` owned the reads, policy, credential copies and the
  `CLAUDE_CODE_OAUTH_TOKEN` secret read from the daemon env; `prepare_persistent_claude_home`
  seeded `claude_cli.claude_home_dir` (default `~/.tddy/sandbox-claude-home`), mounted rw.
- `build_sandbox_runner_env` added the Claude overlay to every agent; the jail's egress was `any`.
- The sandboxed start dropped `session_role`, so a project-home main was refused `sandbox`.

## State B

- **Generic jail sections on `ToolDef`** (`tddy_actions::tool`): `HomeSection::{Persistent { key }, Session}`,
  `FileItem::{SeedOnce, Mirror, Overlay}`, `SecretItem { env, source: HostEnv | HostFile, when: Option<RouteGuard> }`,
  `PolicySection { jit, fork, exec_paths, sysctl_read }` — all `#[serde(default)]`.
- **Lowering** (`tddy_sandbox_recipes`): `tool_grants::{lower_home, lower_files, lower_secrets, lower_policy}`
  (`JailPaths`, `HomeGrant`, `FileGrants`), and `homes::{persistent_home, legacy_home, HomeError, HOMES_DIR}`:
  `<tddyhome>/homes/<key>`, renamed once from `<tddyhome>/sandbox-<key>-home`, refused when both
  exist. `lower_files` replaces a directory at a mirror's `to` only when it holds nothing but links.
- **`claude.yaml`** declares `home: { kind: persistent, key: claude }`, the credential seed and install
  mirrors, the `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` overlays, `CLAUDE_CODE_OAUTH_TOKEN` (`host-env`,
  `when: unrouted`), its policy, and `network.hosts` (`api.anthropic.com`, `statsig.anthropic.com`,
  `sentry.io`, `console.anthropic.com`, `claude.ai`) with `oauth_inbound: true`.
- **The runner's generic tool mode** (`tddy_sandbox_runner::exec_plan`): `--exec-plan <file>` runs an
  `ExecPlan { argv, env, files, egress_shim }` — files written, env replaced (no `TDDY_SANDBOX_*`,
  `TDDY_SECRET_*` resolved, proxy env in both casings when a shim runs), the jail ending with the
  process. `run_sandbox_runner_inner` takes the plan; `ChildEnv` on the generic PTY thread.
- **The jail of a tool** (`tddy_daemon_sandbox::tool_jail`): `lower_tool_jail` → `ToolJailGrants`,
  `build_tool_jail_plan`, `route_hosts` — a provider route's base-URL host is admitted to the egress
  policy. `sandbox_session::dial_and_bridge_with_egress` holds the host relay to the jail's
  `EgressGuard`. `tddy_sandbox::SandboxPlan::add_secret`.
- **The shared jail launcher** (`tddy_agent_launch`): `jail_launch.rs` `LaunchSessions::launch_jailed(JailSpec, params, slots, JailedSession, role)`
  (`EXEC_PLAN_FILE`, `mcp_server_env`), and `claude_jail.rs` `launch_jailed_claude(ClaudeJail)`.
  `start_sandboxed_claude_cli_session` and `relaunch_sandboxed_runner` are thin callers; the start
  carries `session_role` (`record_role_before_spawn`, `write_jail_session_metadata`), and
  `refuse_what_main_cannot_honour` no longer refuses `sandbox`. `tddy-agent-launch` depends on
  `tddy-sandbox-recipes` (no cycle: sandbox-recipes depends on actions, core, sandbox and
  workflow-recipes only). `launch_jail` (`jail_launch_steps.rs`), `JailLaunch`, `JailRunnerEnv` and
  `resolve_relaunch_binaries` are gone.
- The agent process sees `HOME=<tddyhome>/homes/claude`, a `PATH` of its declared tools, `HTTPS_PROXY`,
  and no `TDDY_*` variable; its `tddy-tools --mcp` server carries `TDDY_SANDBOX_TOOL_IPC` and
  `TDDY_SANDBOX_SESSION_ID`. No Claude flag reaches the runner from the daemon.

## Facts worth keeping

- **The home moved.** daemon.yaml `claude_cli.claude_home_dir` and `TDDY_SANDBOX_CLAUDE_HOME` no longer
  decide the daemon's jailed Claude home: it is `<tddyhome>/homes/claude`, migrated once by `rename`
  from `<tddyhome>/sandbox-claude-home`. A custom `claude_home_dir` elsewhere is not migrated; its
  credentials are re-seeded from the host's `~/.claude/.credentials.json` (seed-once).
  `tddy-sandbox-app` still uses its own `claude_home_dir`.
- **Kept, uncalled.** `tddy_daemon_sandbox::sandbox_session::prepare_persistent_claude_home` has no
  caller after this change. `SandboxRecipe::ClaudeCli`, the runner's Claude flags and that function
  stay for `tddy-sandbox-app` and split sessions; deleting them is `#agent-tools` 10/12
  ([#799](https://github.com/uppin/tddy-coder/pull/799), jail-agnostic).
- **No `--strict-mcp-config` in the jail.** The jailed MCP slot is rendered without the strict flag,
  keeping the old launch's behaviour: a jailed agent loads the project's `.mcp.json` beside
  `tddy-tools`. Open decision:
  [2026-11-02-jailed-claude-loads-project-mcp-json](../todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)
  (`TODO(claude-jail)` in `claude_jail.rs`).
- **macOS only.** Every new jail test is `#![cfg(target_os = "macos")]`; the runner's `--exec-plan`
  path is exercised only on Seatbelt, and no CI host runs a Linux jail
  ([2026-11-02-ci-runs-no-linux-jail](../todo/2026-11-02-ci-runs-no-linux-jail.md)).
- **Test edits approved by the developer (2026-11-02).** The red-phase assertion "agent `TDDY_*` names
  equal the `tddy-tools` server's" could not hold beside "no `TDDY_SANDBOX_*` on the agent" (in-jail
  `tddy-tools` finds its socket only through `TDDY_SANDBOX_TOOL_IPC`), so the test is
  `the_agent_process_sees_no_tddy_variable_and_its_tddy_tools_server_carries_the_ones_it_needs`.
  The baseline, the sandboxed-main test and three sandboxed `claude_cli_fullscreen_renderer` tests
  gained a Given with coding agents installed; no assertion changed.
  `setting_up_main_in_a_sandbox_is_refused_and_starts_nothing` is removed, superseded.
- **Linux-only Given, found by CI (`/fix-pr`, 2026-11-02).**
  `sandboxed_claude_cli_acceptance::start_session_sandbox_unsupported_on_non_darwin` (non-macOS
  only, so no local run reached it) was refused with `this host has no coding agents installed`
  before the jail spawn could report `sandbox unsupported`. Its Given now installs coding agents
  over a stub `PATH` (a stub `claude` and the built `tddy-tools`), the same Given edit the developer
  approved above. Its assertion is unchanged, and no production refusal was reordered.
- **Darwin-only Givens (`/fix-pr`, 2026-11-02).** The 12 macOS sandboxed Claude tests in
  `sandboxed_claude_cli_acceptance` (5), `sandbox_behavior_acceptance` (5) and
  `sandboxed_session_lifecycle_acceptance` (2) were refused with `this host has no coding agents
  installed`. Their Givens now install coding agents, with the built `tddy-tools` on a stub `PATH`.
  Linux CI does not run them. They also install the sandbox RPC bridge, as the runtime does at
  mount. Their `minimal_service` never had it, a harness gap since #670.
- **Three tests moved to the no-`TDDY_*`-on-the-agent contract (developer decision, 2026-11-02).**
  Production is unchanged. `tddy_testing_commons::write_egress_probe_claude_script(dir, host, port)`
  bakes the direct-socket target into the script instead of reading `TDDY_EGRESS_PROBE_HOST/PORT`.
  It reaches the shim through `HTTPS_PROXY` (with `--noproxy '*'`), not `TDDY_EGRESS_SHIM`. That
  keeps `sandboxed_session_relays_claude_llm_egress_via_session_channel` and
  `sandboxed_session_denies_direct_outbound_network_from_jail` proving the same behaviour. Its
  other caller, `tddy-sandbox-darwin`'s `sandbox_runner_behavior_acceptance`, passes the target too.
  `sandboxed_claude_cli_start_wires_specialized_agents_env_and_metadata` now asserts
  `TDDY_SUBAGENT=fastcontext` in the `tddy-tools` server's `env` of the `--mcp-config` the agent
  was given, where `tddy-tools` reads it, instead of in the jailed process env.

## Tests

- Coverage first: `tddy-session-lifecycle` `sandboxed_claude_launch_baseline` (2) passed on the old
  launch and stays green on the launcher.
- `claude_cli_jail_from_spec_acceptance` (7, real Seatbelt, fixture `claude_jail_support/`):
  argv/home/path/proxy/MCP/credentials match the spec; no `TDDY_*` on the agent; the relaunch renders
  the same spec; a declared host reached and an undeclared one refused; a routed session reaches its
  base-URL host with no OAuth secret; the legacy home migrated once; no Claude flag on the runner.
- `ensure_project_home_session_acceptance::a_project_home_main_set_up_in_a_sandbox_is_listed_with_session_role_project_home`.
- Unit: `tddy-sandbox-recipes` `persistent_home_unit` (4), `lower_jail_sections_unit` (11),
  `lower_mirror_over_a_seeded_home_unit`; `tddy-sandbox-runner` `exec_plan::tests` (3);
  `tddy-daemon-sandbox` `tool_jail::tests` (3).

## Validation

`/validate-changes`: rebased onto `eae01cf5c`; leak check clean (4 own commits); no parent-owned file
deleted; no Dependencies trespass. Linux self-skips: none needed — every test this change added that
spawns a jail is macOS-only. File splits deferred: `runner.rs` and `builder.rs` are stack-shared;
`sandbox_session.rs` deferred by the developer to the `#agent-tools` sweep node.

## Backlog

- **Resolved and deleted:** *2026-09-24 — the shared sandboxed jail launch (DRY #1) waits on tests
  that actually run the three callers* (`2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first`).
  The shared launcher is built coverage-first and the Claude start and relaunch run on it. The third
  caller, the sandboxed Cursor start, moves onto it in `#agent-tools` 7/12
  ([#796](https://github.com/uppin/tddy-coder/pull/796)), which names that move in its own scope.
- **Narrowed:** [2026-10-24-sandboxed-main-needs-session-role](../todo/2026-10-24-sandboxed-main-needs-session-role.md)
  — the role is carried and `sandbox` is allowed; the main's tool wiring and role prompt in the jail,
  and the role on a relaunch, remain.
- **Added:** [2026-11-02-jailed-claude-loads-project-mcp-json](../todo/2026-11-02-jailed-claude-loads-project-mcp-json.md).
- Kept, touched but not fixed: `2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish` (the
  runner's generic exec mode is extended beside the confined-action mode it covers) and
  `2026-09-12-the-in-jail-conversation-suite-runs-nowhere` (the new live jail tests are macOS-only
  too, stated in their module docs).

## Code issues

All deferred to the `#agent-tools` sweep node (developer, 2026-11-02).

- **Narrowed:** `tddy-agent-launch` `complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session`
  — 349 → 281 lines (the hand-built jail launch is gone; the 24-parameter signature and the set-up
  before the launch remain); claim released.
- **Worse, rows added:** `tddy-daemon-sandbox` `oversized-file-sandbox-session` (946 → 985 production
  lines) and `heavy-params-sandbox-session-dial-and-bridge` (`dial_and_bridge_with_egress`, 12
  parameters); `tddy-sandbox-runner` `oversized-file-runner` (2,799 → 2,847); `tddy-sandbox`
  `oversized-file-builder` (577 → 596).
- **New:** `tddy-agent-launch` `complexity-jail-launch-launch-jailed` (147 lines),
  `complexity-claude-jail-launch-jailed-claude` (80 lines),
  `heavy-params-svc-relaunch-sandboxed-runner-relaunch-sandboxed-runner` (14 parameters);
  `tddy-daemon-sandbox` `complexity-tool-jail-lower-tool-jail` (68 lines); `tddy-sandbox-runner`
  `complexity-runner-run-sandbox-runner-inner` (245 → 261 lines).

Docs: [Claude CLI session § The jail](../../ft/daemon/claude-cli-session.md#the-jail-from-the-claude-tool-definition),
[Sandbox builder § Jail sections](../../ft/coder/sandbox-builder.md#jail-sections),
[project concept](../../ft/daemon/project-concept.md),
[declared tools](../../../packages/tddy-actions/docs/declared-tools.md),
[coding agent specs](../../../packages/tddy-coding-agent-specs/docs/coding-agent-specs.md),
[tddy-agent-launch module layout](../../../packages/tddy-agent-launch/docs/module-layout.md#the-sandboxed-starts-and-relaunch),
[tddy-daemon-sandbox § tool_jail](../../../packages/tddy-daemon-sandbox/docs/task-and-action-services.md#tool_jail),
[tddy-sandbox architecture § `--exec-plan`](../../../packages/tddy-sandbox/docs/architecture.md),
[tddy-session-lifecycle test suites](../../../packages/tddy-session-lifecycle/docs/test-suites.md).
