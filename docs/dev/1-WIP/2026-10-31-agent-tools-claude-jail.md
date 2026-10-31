# Changeset: A sandboxed Claude CLI session is configured by its tool definition

**Created:** 2026-10-31
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — five or more crates (`tddy-actions`, `tddy-coding-agent-specs`, `tddy-sandbox-recipes`, `tddy-sandbox-runner`, `tddy-agent-launch`, `tddy-daemon-sandbox`); a data migration (persistent home); a new runner mode; coverage-first replacement of a 342-line launch; live egress
**Stack:** `#agent-tools 5/12` — branch `feature/agent-tools/claude-jail`, base `feature/agent-tools/build-tools`
**PR:** [#794](https://github.com/uppin/tddy-coder/pull/794)
**PRD:** [PRD-2026-10-31-claude-cli-jail-from-spec.md](../../ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md)

## Initial Discovery

[2026-10-31-agent-tools-claude-jail-initial-discovery.md](./2026-10-31-agent-tools-claude-jail-initial-discovery.md)

State A below is distilled from that file. Grep traces and file dumps are not repeated here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md](../todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) | ✅ **RESOLVED HERE.** This node builds the shared jail launcher, coverage-first as the entry requires, and moves the Claude start and the relaunch onto it. #796 cursor-cli moves the third caller, and references this entry without claiming it |
| [2026-10-24-sandboxed-main-needs-session-role.md](../todo/2026-10-24-sandboxed-main-needs-session-role.md) | ✅ **RESOLVED HERE.** The launcher carries `session_role`, so `record_role_before_spawn` writes it into the jailed session and resume re-applies it. `sandbox` is dropped from `refuse_what_main_cannot_honour` |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` | ✅ **RESOLVED HERE.** The function is replaced by the launcher. Claimed at commit 2 |
| [2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish.md](../todo/2026-09-19-action-sandbox-acceptance-pty-test-does-not-finish.md) | ⚠ During. The runner's generic exec mode is extended here. Do not regress it, and re-measure it at wrap |
| [2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md](../todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md) | ⚠ During. The new live jail tests are macOS-only too. Say so in their module docs |
| `packages/tddy-sandbox-runner/docs/code-issues/oversized-file-runner.md` | ⚠ During. `--exec-plan` lives in a new `exec_plan.rs` |

## Affected Packages

- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): generic `ToolDef` sections `home`, `files`, `secrets` and `policy`
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `claude.yaml` gains `home`, `files`, `secrets`, `policy` and `network`; the slots render jail paths
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md): lowering of home, files, secrets and policy (`tool_grants`); `<tddyhome>/homes/<key>` plus the one-time migration
- [ ] [`tddy-sandbox-runner`](../../../packages/tddy-sandbox-runner/README.md): `--exec-plan <file>`: a generic exec of a rendered argv, env and files with proxy env and `TDDY_SECRET_*` resolution
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): the shared jail launcher; the sandboxed claude-cli start and relaunch use it; `session_role` is carried
- [ ] [`tddy-daemon-sandbox`](../../../packages/tddy-daemon-sandbox/README.md): a plan from a spec-built request; the route base-URL host joins the egress policy

## Responsibility

- `tddy_actions::tool::{HomeSection::{Persistent { key }, Session}, FileItem::{SeedOnce { from_host, to }, Mirror { from_host, to }, Overlay env}, SecretItem { env, source: HostEnv | HostFile }, PolicySection { jit, fork, exec_paths, sysctl_read }}`.
- `tddy_sandbox_recipes::tool_grants::{lower_home, lower_files, lower_secrets, lower_policy}`, plus `homes::persistent_home(tddy_home, key) -> PathBuf` with a one-time `rename` from `sandbox-<key>-home`.
- `tddy_sandbox_runner::exec_plan::{ExecPlan { argv, env, files, egress_shim: bool }, run_exec_plan}`. Proxy env and secret resolution are applied exactly as the agent paths do today.
- `tddy_agent_launch::jail_launch::launch_jailed(spec, params, slots, session, role) -> Result<…>`, used by `start_sandboxed_claude_cli_session` and `relaunch_sandboxed_runner`, which keep their names as thin callers.
- `claude.yaml`: `home: { kind: persistent, key: claude }`, credential seed and install mirrors, `CLAUDE_CODE_TMPDIR`/`CLAUDE_TMPDIR` overlays, `CLAUDE_CODE_OAUTH_TOKEN` (`HostEnv`, unrouted only, through a `when: unrouted` guard), its policy, and `network.hosts` with `oauth_inbound: true`.
- Route base-URL host joins the egress policy at plan time.

## Boundaries

- Does not delete `SandboxRecipe::ClaudeCli`, the runner's Claude flags or `prepare_persistent_claude_home`. `tddy-sandbox-app` and split sessions still use them (#799).
- Cursor, OpenCode, Muse and Warp jails are untouched (#796, #797).
- No headless or ACP.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-defs (#790) | `ToolDef`, registry, `resolve_closure`, `lower_tools` | the jail's exec surface for `claude` plus `tddy-tools` | change `lower_tools` or `rg.yaml` |
| `n2` claude-host | `run`/`render_argv`, `extensions`, `CodingAgentSpec`, slots `render`, `claude.yaml`, `tddy-tools.yaml` | renders Claude's argv and slots for the jail; extends `claude.yaml` with jail sections | change the template grammar, the slot kinds, or `launch_host_pty` |
| `n3` egress-allowlist | `NetworkSection`, `EgressPolicy`, `lower_network`, shim and relay enforcement | Claude's declared hosts become the jail's policy; tests assert an undeclared host is refused | change the union rule or `admits` |
| `n4` build-tools | nothing consumed | sits above #793 only because of the line | touch `tddy-build` |

## Draft PR contract

The first push publishes:

- the four generic sections' types and their `lower_*` signatures;
- `persistent_home`;
- `ExecPlan` and `run_exec_plan`, plus the `--exec-plan` flag;
- the `launch_jailed` signature;
- `claude.yaml`'s jail sections;
- the failing tests below.

The coverage-first acceptance tests pinning **today's** sandboxed Claude start and relaunch are written first, and they pass on today's code. They are the baseline the launcher must keep green. Unimplemented bodies are `// TODO(claude-jail): implement`.

## Green wave

**Wave:** 3 of 7
**Greenable independently:** only after #791 (rendering, slots) and #792 (egress enforcement) are green. Its tests run the real launcher and the real egress refusal
**Concurrent with:** #795 claude-headless
**Blocks:** #796 cursor-cli and #797 harness-clis (they reuse `launch_jailed` and the generic sections), #799 jail-agnostic

Real dependency edges, as opposed to the branch line:

    n1 → n2, n3, n4      n2 → n5, n6      n3 → n5, n7, n8      n5 → n7, n8, n10
    n6 → n9      n7 → n9, n10      n8 → n10, n11      n9 → n11      n2..n11 → n12

## State A

- `svc_start_sandboxed_claude_cli_session.rs` (342 lines), `svc_relaunch_sandboxed_runner`, and the staged helpers listed in the lifecycle todo.
- Runner flags `--claude-binary`, `--claude-arg`, `--permission-mode`, `--dangerously-skip-permissions`, `--resume` and `--append-system-prompt-file`. The runner builds Claude's argv (`build_claude_base_argv`) and writes `$TMPDIR/claude-mcp-config.json`.
- `SandboxRecipe::ClaudeCli` owns the reads, policy, credential copies and the `CLAUDE_CODE_OAUTH_TOKEN` secret from the daemon env, and sets `allow_oauth_inbound` when unrouted.
- `prepare_persistent_claude_home` (seed-once credentials, local-install mirror) lives at `~/.tddy/sandbox-claude-home`, which is mounted rw.
- `build_sandbox_runner_env` always adds the Claude overlay.
- The sandboxed claude-cli start drops `session_role`.

## State B

- The sandboxed claude-cli start and the relaunch call `launch_jailed`. It resolves the `claude` closure, renders argv and slots for jail paths, lowers home, files, secrets, policy, network and tools into the plan, and spawns the runner with `--exec-plan`.
- Claude sees `HOME=<homes/claude>`, a `PATH` of declared tools, `HTTPS_PROXY` and its config files where it expects them.
- No Claude flag reaches the runner from the daemon.

## Delta

### New
- `packages/tddy-actions/src/tool/{home.rs, files.rs, secrets.rs, policy.rs}`
- `packages/tddy-sandbox-recipes/src/homes.rs`
- `packages/tddy-sandbox-runner/src/exec_plan.rs`
- `packages/tddy-agent-launch/src/jail_launch.rs`
- Tests: `packages/tddy-session-lifecycle/tests/{claude_cli_jail_from_spec_acceptance.rs, sandboxed_claude_launch_baseline.rs, claude_jail_support/mod.rs}`, `packages/tddy-sandbox-recipes/tests/{persistent_home_unit.rs, lower_jail_sections_unit.rs}`
### Modified
- `claude.yaml`; `tool_grants.rs`; the runner's `main.rs` (`--exec-plan`). `tddy-sandbox-recipes` gains `thiserror` and `tddy-sandbox-runner` gains `serde` — both already workspace dependencies.
- `packages/tddy-session-lifecycle/tests/project_home_support/mod.rs`: the service is an `Arc` with its sandbox RPC bridge installed.
- `svc_start_sandboxed_claude_cli_session.rs` and `svc_relaunch_sandboxed_runner.rs` become thin callers.
- `project_home_start_spec.rs`: the `TODO` and `sandbox` are removed from the refused placements.
- `tddy-daemon-sandbox` `sandbox_session.rs` builds from a spec request.
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md` § Sandbox mode; `docs/ft/coder/sandbox-builder.md`; `packages/tddy-sandbox-runner/docs`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_jail_from_spec_acceptance.rs`** (whole file macOS-only: every test runs a real Seatbelt jail; fixture in `tests/claude_jail_support/mod.rs`)

- [x] `a_sandboxed_claude_cli_start_runs_a_stub_claude_whose_argv_home_path_proxy_env_mcp_config_and_credentials_match_the_spec` (macOS, real Seatbelt)
- [x] `the_agent_process_needs_no_tddy_sandbox_variable_and_sees_only_the_tddy_vars_its_declared_tddy_tools_server_needs`
- [x] `a_relaunch_uses_the_same_launcher_and_renders_the_same_spec`
- [x] `claude_in_a_jail_reaches_a_declared_host_through_the_shim_and_is_refused_an_undeclared_one` (local stand-in hosts mapped through the pattern list)
- [x] `a_routed_session_reaches_its_base_url_host_and_carries_no_oauth_token_secret`
- [x] `an_existing_sandbox_claude_home_is_migrated_to_homes_claude_once_with_its_credentials`
- [x] `the_runner_receives_no_claude_specific_flag_on_the_daemon_path`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` (extended)**

- [x] `a_project_home_main_set_up_in_a_sandbox_is_listed_with_session_role_project_home`

**Coverage-first baseline: `packages/tddy-session-lifecycle/tests/sandboxed_claude_launch_baseline.rs`** (macOS-only). These two are the **exception to "every new test fails"**: they pass on today's code at commit 2 and are the baseline the launcher must keep green.

- [x] `todays_sandboxed_claude_start_records_argv_env_mounts_and_files` (passes on today's code and stays green across the replacement)
- [x] `todays_sandboxed_relaunch_records_the_same`

### Unit / integration tests

- [x] `tddy-sandbox-recipes` `homes` (`tests/persistent_home_unit.rs`): the migration renames once, is idempotent, and refuses when both the old and new directories exist (naming both).
- [x] `tddy-sandbox-recipes` `tool_grants` (`tests/lower_jail_sections_unit.rs`, which also pins `lower_home` and `lower_policy`): `SeedOnce` copies only when absent; `Mirror` produces symlinks; an `Overlay` env is joined; `HostEnv` secrets become `SecretSpec`; a `when: unrouted` guard drops the secret when a route is set.
- [x] `tddy-sandbox-runner` `exec_plan` (inline `exec_plan::tests`, on `ExecPlan::child_env`): the proxy env is set in both casings when the shim is on and absent when off; `TDDY_SECRET_*` files are resolved to their real names and unlinked.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Baseline of today's launch | Integration with a recording stub runner | Coverage first, as the lifecycle todo requires |
| Jail behaviour | Live macOS Seatbelt | An unaware agent is only provable in the real jail |
| Lowering, home migration, exec plan | Unit | Pure, or temp dir |

## Decisions & Trade-offs

- **Coverage first.** The baseline tests land in commit 2 and pass on today's code, then the launcher replaces the code under them.
- **Persistent homes are generic** (`homes/<key>`), not Claude-named, and are migrated once with `rename` rather than copied.
- **`when: unrouted` guards** on spec items express the routing split, replacing the recipe's hidden daemon-env passthrough.
- **The recipe arm stays until #799.** Two consumers remain, and deleting it here would put #799's work in this diff.
- Complexity 5: say so to whoever greens it, and consider two milestones, the coverage baseline and then the launcher.

## TODO

- [x] Record initial discovery (`2026-10-31-agent-tools-claude-jail-initial-discovery.md`)
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

**Published surface (commit 2).**

- `tddy_actions::tool::{HomeSection, FileItem, SecretItem, SecretItemSource, RouteGuard, PolicySection}` (`src/tool/{home,files,secrets,policy}.rs`), and `ToolDef.{home, files, secrets, policy}` — all `#[serde(default)]`, additive. These are plain serde types with no bodies to implement.
- `tddy_sandbox_recipes::homes::{persistent_home, legacy_home, HomeError, HOMES_DIR}`; `tool_grants::{lower_home, lower_files, lower_secrets, lower_policy, JailPaths, HomeGrant, FileGrants}`.
- `tddy_sandbox_runner::exec_plan::{ExecPlan, ExecPlanFile, ExecPlan::child_env, run_exec_plan}`, and the runner binary's `--exec-plan <file>` flag (on `main.rs`'s `Cli`, beside `--workspace-tools`, so no `SandboxRunnerArgs` literal elsewhere changes). Nothing passes the flag yet.
- `tddy_agent_launch::jail_launch::{LaunchSessions::launch_jailed, JailSpec, JailedSession, EXEC_PLAN_FILE}`.
- `claude.yaml`: `home`, `files`, `secrets` and `policy` sections, appended after n2's sections, which are unchanged. The `network:` section is **written but commented out** with `TODO(claude-jail)`: `Egress::deserialize` is still `todo!(egress-allowlist)`, and a `network:` key would make every load of `claude.yaml` — n2's unsandboxed launch once it greens — panic on this branch. Green uncomments it.
- Today's path is untouched: `start_sandboxed_claude_cli_session` and `relaunch_sandboxed_runner` still build the runner's Claude argv, each with a `TODO(claude-jail)` at the spot green replaces with `launch_jailed`; `project_home_start_spec.rs`'s TODO is now `TODO(claude-jail)`.

**Red baseline.**

- Unit, all failing on this node's `todo!`: `persistent_home_unit` 4/4 (`claude-jail: persistent_home`); `lower_jail_sections_unit` 11/11 (`lower_home` 2, `lower_files` 4, `lower_secrets` 4, `lower_policy` 1); `exec_plan::tests` 3/3 (`ExecPlan::child_env`).
- `sandboxed_claude_launch_baseline` **2/2 pass** on today's code (the coverage-first exception). The jail home is configured at `<tddyhome>/homes/claude`, which is where `persistent_home` puts Claude's, so the same assertions hold across the replacement.
- `claude_cli_jail_from_spec_acceptance` 7/7 fail. **As committed they fail in their Given**, at `todo!("claude-host: load_agent_registry")`: the fixture gives the host its coding agents as the runtime does, which is n2's (#791) surface. That is a sequencing fact, already in `## Green wave` (greenable only after #791 and #792). To check the assertions themselves, the suite was run once with the coding-agents install switched off (not committed); every test then failed on **this node's** behaviour: HOME is `configured-claude-home`, not `homes/claude`; the agent sees `TDDY_SANDBOX_{EGRESS_DIR,SESSION_ID,TOOL_IPC}`; no `exec-plan.json` is written; the undeclared stand-in host answers `200`, not `403`; a routed session reaches an undeclared host and the unrouted control session has no token; the legacy home stays at `sandbox-claude-home`; the runner argv carries `--claude-binary` and `--permission-mode` and no `--exec-plan`.
- `ensure_project_home_session_acceptance::a_project_home_main_set_up_in_a_sandbox_is_listed_with_session_role_project_home` fails: the start is refused with `InvalidArgument` (`may not set sandbox`). The other 29 tests in the file pass, as do `project_home_idle_stop_acceptance` (5) and `ensure_project_home_session_replace_edges` (8) after the shared fixture change below.

**Pre-existing failures seen (not this node's).** Parent contract-stage reds in the touched packages: `tddy-actions` (`tool-defs`, `egress-allowlist`, `claude-host` todos), `tddy-coding-agent-specs` (`claude-host`), `tddy-sandbox-recipes` `tool_grants_unit`/`lower_network_unit`, `tddy-sandbox-runner` `egress_allowlist_relay_acceptance` and 3 lib tests (`egress-allowlist`), `tddy-agent-launch` `spec_launch_acceptance` (`claude-host`). `tddy-agent-launch` `clone_askpass_acceptance` 9 fail because `target/debug/tddy-askpass` is not built by a bare `cargo test` (harness). `sandboxed_claude_cli_acceptance` fails on this Mac because its `minimal_service` never calls `install_sandbox_rpc_bridge` (it panics at `svc_host_builders.rs:151`) — pre-existing, and why this node's fixture installs the bridge.

**Deviations from the plan.**

- `persistent_home` returns `Result<PathBuf, HomeError>`, not `PathBuf`: the refusal when both homes exist has to be a value.
- `launch_jailed` is a `pub async fn` **method on `LaunchSessions`**, not a free function: ready, bridge and register need the session host's sandbox manager, task registry and activity hub. Its `role` is `&str` (as `record_role_before_spawn` takes it).
- `FileItem::Overlay { env, value }`: `value` may name `{home}` and `{tmp}`. `SecretItem.when` is a `RouteGuard` (`unrouted` | `routed`). `HomeSection` defaults to `Session`; `PolicySection` defaults to no allows.
- The runner's exec-plan observable is a file, `<session>/sandbox/exec-plan.json` (`EXEC_PLAN_FILE`), which the acceptance tests read to compare the rendered plan with what the stub saw.
- The shared fixture `tests/project_home_support/mod.rs` now holds the service as `Arc<DaemonSessionHost>` and installs its sandbox RPC bridge, as the daemon runtime does, so a sandboxed main's jail can reach it.
- The egress and routed tests map local stand-in hosts through a `<tddyhome>/tools/claude.yaml` override (the built-in parsed and its `network`/`secrets` replaced). The routed test reads the OAuth token from a `host-file` secret rather than the daemon's environment, so no test mutates process env.

**For green (developer decisions flagged).**

- Green **removes** `ensure_project_home_session_acceptance::setting_up_main_in_a_sandbox_is_refused_and_starts_nothing` (marked `TODO(claude-jail)`): the new sandboxed-main test supersedes it. It is a test removal, not a file deletion.
- The baseline builds its daemon **without** coding agents (installing them panics on n2's todo today). If the launcher requires them, green adds them to the baseline's Given — no assertion changes.
- `claude_cli.claude_home_dir` and `TDDY_SANDBOX_CLAUDE_HOME` stop deciding where the jailed Claude's home is (`persistent_home` does). Green should say what happens to them; the migration test configures `claude_home_dir` at the legacy path so today and tomorrow are both observable.
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
