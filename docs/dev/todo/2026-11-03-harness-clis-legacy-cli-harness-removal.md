# 2026-11-03 — The legacy `CliHarness` table and the jail's `--agent-kind` helpers outlive the harness-clis launch

**Category:** Deferred from `#agent-tools 8/12` (harness-clis, PR #797)
**Source:** the harness-clis changeset, wrapped as `docs/dev/changesets/2026-11-03-agent-tools-harness-clis.md`
(`## Refactoring Needed` → `### From /red`, the `TODO(...)` deletions needing approval) and the green
pass that moved the OpenCode, Muse Code and Warp launches onto their specs.

**Decided:** 2026-11-03, by the developer. Deleting the legacy code is deferred, not dropped.

## What changed, and what did not

The launch no longer reads the `CliHarness` descriptors. `opencode-cli`, `muse-cli` and `warp-cli`
start and resume from their tool definitions (`tddy-coding-agent-specs/specs/{opencode,muse,warp}.yaml`):

- a host start renders the spec's `run:` argv through `launch_host_pty`;
- a jailed start and resume go through `launch_jailed` over the same spec;
- the route is the spec's `route` section, the resume token is read by the spec's `resume` adapter,
  and Warp's jail refusal is its spec's `placements`.

A session type with no spec is refused. There is no fallback to the table.

The table itself still exists, and so do the modules and helpers listed below, because other code
still reads them. Deleting them in #797 would widen that PR beyond the launch switch.

## Why deferred

Production code outside the launch path still reads the table, and tests pin the legacy helpers:

- **Table consumers that are not part of the launch switch:**
  - `tddy-agent-launch/src/facilitation.rs:33` and `session_coordinate_handlers.rs:188` (`cli_harness(..).is_some()`);
  - `tddy-agent-launch/src/start_request_checks.rs` (`cli_harness_requires_model`, and the legacy refusal);
  - `tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` (the agent tab's argv; its binary
    comes from the spec since `#agent-tools 19/19`);
  - `tddy-session-activity/src/session_list_enrichment.rs`, `tddy-session-files/src/context_files.rs`;
  - `tddy-daemon-rpc/src/catalog/ports.rs`, `tddy-tool-engine/src/start_session_args/schema.rs` (`cli_harnesses()`);
  - `tddy-cli-sessions/src/cli_session_manager/{argv,launch}.rs`;
  - `tddy-sandbox-recipes/src/route_files.rs` (`route_config_env(recipe)` reads the table for the app's
    `SandboxRecipe::{OpenCodeCli, MuseCli}`, which stay until #799);
  - `tddy-sandbox-runner/src/harness_pty.rs` (the runner's `--agent-kind` PTY hosting).
- **Pinned tests** (listed below), which build the descriptors and call the legacy helpers directly.

## Files to delete

Each is marked `TODO(harness-clis)` in its source.

- `packages/tddy-agent-backend/src/cli_harness.rs`: the `CLI_HARNESSES` table, `CliHarness`,
  `CliLaunch`, `CliModelSource`, `cli_harness()`, `cli_harnesses()`, `cli_harness_requires_model()`.
- `packages/tddy-agent-backend/src/cli_harness_muse.rs`: `MUSE_CLI` (its data is `specs/muse.yaml`).
- `packages/tddy-agent-backend/src/cli_harness_warp.rs`: `WARP_CLI` (its data is `specs/warp.yaml`).

## Legacy helpers to remove with them

Each carries `#[allow(dead_code)]` and a `TODO(harness-clis)` marker until it goes. The allows are
per item, never module-wide.

- `packages/tddy-agent-launch/src/cli_harness_jail.rs`, below its `--agent-kind` line:
  `sandboxed_agent_kind`, `require_sandbox_hosts`, `sandboxed_runner_model`, `model_argument`,
  `refuse_first_prompt`, `require_provider_route`, `resolve_jail_agent_binary`, `CliJailBinaries`,
  `resolve_cli_jail_binaries`, `CliRunnerArgv`, `cli_harness_runner_argv`.
- `packages/tddy-agent-launch/src/opencode_cli_spawn.rs`: `harness_binary` (used only by
  `resolve_cli_jail_binaries`).
- `packages/tddy-agent-launch/src/start_request_checks.rs`: `refuse_an_unsupported_cli_harness_start`
  (superseded by `refuse_an_unsupported_cli_session_start` and `LaunchSessions::require_sandbox_placement`).
- `packages/tddy-agent-launch/src/relaunch_jail_steps.rs`: `relaunch_jail_env`, `spawn_relaunched_runner`,
  `bridge_relaunched_jail`, with their structs `RelaunchJailEnv`, `RelaunchedRunnerSpawn` and
  `RelaunchedJailBridge` in `svc_relaunch_sandboxed_runner.rs`. The CLI-agent resume relaunches through
  `launch_jailed`, so nothing calls them.
- `packages/tddy-cli-sessions/src/cli_session_manager/launch.rs`: `CliSessionManager::launch_cli_harness`,
  which takes a `&CliHarness`. The host start no longer calls it.

## Pinned tests to migrate

These keep compiling and passing until they move onto specs. None is edited for this deferral.

- `packages/tddy-agent-backend/tests/cli_harness_acceptance.rs`, `muse_cli_acceptance.rs`,
  `warp_cli_acceptance.rs`: they assert the descriptors' argv, route and model listing.
- `packages/tddy-agent-launch/src/sandboxed_cli_harness_tests.rs`: pins the `--agent-kind` helpers.
- `packages/tddy-agent-launch/src/cli_harness_start_checks_unit_tests.rs`: pins the legacy start refusal.
- `packages/tddy-cli-sessions/tests/cli_harness_parity.rs`, `exit_hook.rs`: build `CliHarness` launches.
- `packages/tddy-tool-engine/tests/start_session_schema.rs`: reads `cli_harnesses()` for the schema.
- `packages/tddy-agent-backend/src/adapters/opencode_models.rs` and `backend/model_catalog.rs`: the model
  listing that `specs/opencode.yaml`'s `models` section now carries.

## What would close it

1. Move every table consumer above onto `CodingAgentRegistry::for_session_type` and the spec's sections
   (`route`, `models`, `resume`, `placements`, `binary`). The descriptor's `route_harness` field is gone
   (`#agent-tools` 13/19): a route is built from the spec's `RoutedAgent`.
2. Move `materialise_route_files` and `route_config_env` in `tddy-sandbox-recipes` onto the spec's
   `route.files_dir` and `route.config_env`. Today the jail writes `provider-route` by convention, and the
   launch sets the configuration variable itself (`cli_harness_jail::jail_route_config_env`). The app's
   `SandboxRecipe::{OpenCodeCli, MuseCli}` keep the table until #799.
3. Migrate the pinned tests above onto the specs, one suite at a time.
4. Delete the three modules and the listed helpers, and remove their `#[allow(dead_code)]` markers with them.
5. The `daemon.yaml cli_harnesses.<type>.binary_path` override stays until #801 (`#agent-tools 12/12`).

Separate deferrals in the same specs are not closed by this entry: the `hooks: unsupported`
amendment to `HooksSlot` (`TODO(harness-clis)` in `specs/{opencode,muse,warp}.yaml` and
`slots/hooks.rs`), and the Warp jail (`docs/dev/todo/2026-10-24-warp-cli-sandbox.md`).

## Narrowed by `#agent-tools` 19/19 (`agent-config-removal`, #825)

The daemon.yaml side of the table is gone: `DaemonConfig::cli_harness_binary`, `CliHarnessConfig` and
`DaemonConfig.cli_harnesses` are deleted. An agent terminal tab (`svc_terminal_ports.rs`) and the
catalogue's model probe (`catalog/ports.rs`) resolve the harness's binary through the host's coding
agents (the spec's `binary.env`, then the host search, refused when found nowhere); only the argv and
the model source still come from the table. `harness_binary` (`opencode_cli_spawn.rs`) reads the
environment, then the descriptor's default — no config step. `cli_harness.rs`'s `requires_model`
copy is now marked `TODO(harness-clis)` (it was `TODO(agent-config-removal)`). The rest of this
record stands; the sweep node (n20) owns it.
