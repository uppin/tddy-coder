# Changeset: One host coding-agent registry

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 5 — re-scored at the contract (was 4): the `DaemonSessionHost::new` signature reaches ~117 constructor call sites in 116 files across 4 crates (session-lifecycle, daemon-rpc, daemon, and tddy-telegram-control's tests), not ~10; plus the `list-models --tools-dir` CLI surface and a new sandbox-app refusal
**PR**: [#830](https://github.com/uppin/tddy-coder/pull/830) — `feature/agent-tools/host-registry`

## Initial Discovery

[2026-11-06-one-host-coding-agent-registry-initial-discovery.md](./2026-11-06-one-host-coding-agent-registry-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md](../todo/2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md) | ✅ RESOLVED HERE (three loads, not four, at `e6ff7012e`) |
| [2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md](../todo/2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md) | ✅ RESOLVED HERE (developer chose to filter, 2026-11-06) |

No `Claimed by:` record in any touched package.

## Affected Packages

- **tddy-daemon** — `runtime/telegram_wiring.rs`, `mount_host.rs`, `mount_accounts.rs`: one fallible load, one `Arc`.
- **tddy-session-lifecycle** — `DaemonSessionHost.coding_agents` non-optional; `svc_coding_agents_wiring.rs`, `svc_resolve_os_user.rs`, `svc_activity_ports.rs`, `svc_terminal_ports.rs`, `daemon_rpc_handler.rs`, `svc_host_builders.rs`.
- **tddy-daemon-rpc** — catalog model probe passes the host's tools dir.
- **tddy-tools**, **tddy-tools-cli** — `list-models --tools-dir`.
- **tddy-sandbox-app** — `choose_agent` offers PTY-runnable specs only.

## Related Feature Documentation

- PRD: [PRD-2026-11-06-one-host-coding-agent-registry.md](../../ft/daemon/1-WIP/PRD-2026-11-06-one-host-coding-agent-registry.md)

## Summary

The daemon loads its coding-agent registry once and fails start-up if it cannot; that one registry
serves the session host, Telegram, the reserved-ids step and the model probe. No production path
turns a missing registry into an empty one. The PTY sandbox offers only agents it can run.

## Scope

- [ ] One fallible host-registry load in runtime wiring, shared by Telegram, reserved ids and the session host
- [ ] `DaemonSessionHost.coding_agents: Arc<HostCodingAgents>`; empty fallbacks deleted
- [ ] Model probe lists from the host registry (operator overrides honoured)
- [ ] `choose_agent` filters to specs with a PTY `run`; refusal names the runnable agents

## Technical Changes

### State A

- Loads: `tddy-daemon/src/runtime/telegram_wiring.rs:90-105` (`TODO(sweep)`, logs and uses an empty registry; called :203), `mount_host.rs:63`, `mount_host.rs:269-270`.
- `tddy-session-lifecycle/src/connection_service.rs:240` `Option<Arc<HostCodingAgents>>`, default `None` (`svc_host_builders.rs:125`); getters `svc_coding_agents_wiring.rs:24,31-36,43-50`. Empty fallbacks: `daemon_rpc_handler.rs:288-290`, `svc_resolve_os_user.rs:76-80`, `svc_activity_ports.rs:87-90`. Option pass-through: `svc_terminal_ports.rs:123,175`, `tddy-daemon/src/runtime/mount_accounts.rs:69` → `account_linking.rs:266`.
- Probe: `tddy-daemon-rpc/src/catalog/ports.rs:114-123,201-213` uses the host registry only for the binary; `tddy-tools-cli/src/main.rs:289-335` lists from `CodingAgentRegistry::builtin_cached()`.
- `tddy-sandbox-app/src/agent_spec.rs:113` `choose_agent`; pinned by `tests/agent_from_spec_acceptance.rs:248-273`.

### State B

- `Registries::open` (or just before it) loads `HostCodingAgents` once; `?` on error; the `Arc` goes to all three consumers.
- `DaemonSessionHost::new` takes the registry; `with_coding_agents_if_none` deleted; tests pass `test_util::builtin_coding_agents()` explicitly. `launch_coding_agents()` returns `&Arc<…>`.
- `list-models --tools-dir <dir>` loads `CodingAgentRegistry` from built-ins + that dir; the daemon probe passes `<tddy home>/tools`.
- `choose_agent` and its `known` list keep specs whose `run` is a PTY launch.

### Delta

| Package | Change |
|---|---|
| tddy-daemon | single load + shared `Arc`; delete Telegram's empty branch |
| tddy-session-lifecycle | non-optional field; builder takes it; delete fallbacks; re-wire tests |
| tddy-daemon-rpc | probe argv adds `--tools-dir` |
| tddy-tools / tddy-tools-cli | `--tools-dir` flag |
| tddy-sandbox-app | filter + refusal; update the pinning test |

## Testing Plan

Integration tests at the daemon runtime and the `tddy-tools` CLI; unit tests in sandbox-app. A
counting or injectable loader proves the single load (`Arc::ptr_eq` between Telegram's and the host's).

## Acceptance Tests

| Test | File:line | Validates | At the contract |
|---|---|---|---|
| `a_daemon_with_an_unparseable_tool_spec_refuses_to_start_naming_the_file` | `packages/tddy-daemon/tests/host_registry_load.rs:17` | a daemon with a Telegram bot and no sign-in, with a broken `tools/claude.yaml`, refuses to start naming the file | 🔴 fails — builds Ok (Telegram's empty-registry branch) |
| `telegram_and_the_session_host_share_one_registry` | `packages/tddy-daemon/tests/host_registry_load.rs:46` | Telegram's registry is `Arc::ptr_eq` the session host's | 🔴 fails — separate loads |
| `list_models_honours_a_tools_dir_override_of_the_curated_models` | `packages/tddy-tools-cli/tests/list_models_tools_dir.rs:14` | `tddy-tools list-models --agent claude-cli --tools-dir` lists the override's models and default | 🔴 fails — lists the built-ins |
| `the_catalog_probe_lists_the_hosts_override_not_the_builtins` | `packages/tddy-daemon-rpc/src/catalog/agent_models/probe_tests.rs:147` | the probe argv carries `--tools-dir <tddy home>/tools` | 🔴 fails — flag missing |
| `the_pty_sandbox_offers_only_agents_with_a_pty_run` | `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs:258` | the `Unknown` refusal lists only PTY-runnable agents (replaces the old pinning test) | 🔴 fails — lists claude-acp, codex-acp |
| `choosing_an_acp_only_agent_in_the_pty_sandbox_is_refused_naming_the_runnable_ones` | `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs:276` | claude-acp / codex-acp → `NotPtyRunnable { id, runnable }` | 🔴 fails — `Placement(NotPlaced)` |

Unit/integration tests on the surface:

| Test | File:line | At the contract |
|---|---|---|
| `list_models_refuses_a_tools_dir_that_does_not_load_naming_the_file` | `packages/tddy-tools-cli/tests/list_models_tools_dir.rs:70` | 🔴 fails — exits 0 |
| `a_listed_agents_probe_carries_its_binary_and_the_hosts_tools_dir` | `packages/tddy-daemon-rpc/src/catalog/agent_models/probe_tests.rs:169` | 🔴 fails |
| `a_daemon_without_a_telegram_bot_offers_no_telegram_registry` | `packages/tddy-daemon/tests/host_registry_load.rs:64` | guard (passes) |
| `the_override_fixture_is_a_claude_spec_with_the_given_models` | `packages/tddy-tools-cli/tests/list_models_tools_dir.rs:43` | guard (passes) |
| `the_acp_refusal_reads_as_the_agent_it_cannot_run_and_the_ones_it_can` | `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs:302` | guard (passes) |
| `list_models_args_carry_the_hosts_tools_dir`, `list_models_args_without_a_tools_dir_name_none` | `packages/tddy-tools/tests/list_models_catalog.rs:98,116` | guards (pass) |
| `a_host_launches_from_the_registry_it_was_constructed_over` | `packages/tddy-session-lifecycle/tests/host_coding_agents.rs:15` | guard (passes) |

Pre-existing failures on this base (not this node's): `catalog::agent_models::probe_tests::{cursor_probe_args_carry_the_resolved_cursor_binary_path, a_non_cursor_probe_omits_the_cursor_cli_path_flag, an_opencode_cli_probe_carries_the_resolved_opencode_binary_path, an_opencode_acp_probe_carries_the_resolved_opencode_binary_path}` (expect the removed per-agent flags; fixed by #828's 50a79f061, not yet on this line); `claude_cli_from_spec_acceptance::an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` (macOS /tmp); parent n21's two `session_source_unit` red tests.

## Responsibility

How the daemon owns its coding-agent registry: one load, one instance, no empty fallback; the model
probe and the PTY sandbox read it.

## Boundaries

- Does **not** thread the registry into split sessions — `feature/agent-tools/split-remote-env`.
- Does **not** add the agent-tool socket registry to `DaemonSessionHost` — `feature/agent-tools/agent-tool-socket-per-user`.
- Does **not** change spec loading or the YAML.

## Dependencies

| Parent | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| #828 | specs-only metadata; `HostCodingAgents`, `builtin_cached()` | wires them once | change spec loading |
| `feature/agent-tools/session-pointer` | branch-line parent only — no surface consumed | — | touch pointer code |

## Draft PR contract

The first push after this plan: `DaemonSessionHost::new(…, coding_agents)` signature, the
`list-models --tools-dir` flag (parsing only), plus the six acceptance tests above, failing.

Published (contract commit):

- `DaemonSessionHost::new(…, cli_manager, coding_agents: Arc<HostCodingAgents>)` —
  `tddy-session-lifecycle/src/connection_service/svc_host_builders.rs`. Stores `Some(coding_agents)`;
  the field stays `Option` and `with_coding_agents_if_none` + the three empty fallbacks stay until
  green (`TODO(host-registry)`). Every caller's chained `.with_coding_agents[_if_none](x)` moved into
  `new(…, x)`; callers with none pass `no_coding_agents()` (behaviour-identical to `None`).
- `RuntimeTasks::telegram_coding_agents() -> Option<&Arc<CodingAgentRegistry>>` (`tddy-daemon/src/runtime/tasks.rs`),
  backed by `TelegramInbound.coding_agents` — the read-only seam the sharing test uses.
- `ListModelsArgs.tools_dir: Option<PathBuf>` (`--tools-dir`), not yet read (`tddy-tools-cli` `agent_models_catalog`, TODO).
- `list_models_probe_args(agent, binary, tools_dir: &Path)` — ignores `tools_dir` (TODO); the handler passes `<tddy_data_dir>/tools`.
- `AgentChoiceError::NotPtyRunnable { id, runnable }` + `Display` — not yet produced (TODO in `choose_agent_on`).

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — no test needs another leftovers node's behaviour
**Concurrent with:** `feature/agent-tools/session-pointer`, `feature/agent-tools/bounded-waits`, `feature/agent-tools/dead-launch-code`
**Blocks:** `feature/agent-tools/agent-tool-socket-per-user` (threads its socket registry beside a non-optional `coding_agents`)

## Decisions & Trade-offs

- **Unconditional load (user, 2026-11-06):** a daemon with no sign-in also refuses a broken `tools/*.yaml` — the load happens before `Registries::open`, whatever is mounted.
- **`NotPtyRunnable { id, runnable }` (user, 2026-11-06)** rather than reusing `Unknown`, whose message would say a registered agent is not in the registry.
- **`RuntimeTasks::telegram_coding_agents()` accessor (user, 2026-11-06)**, no injectable/counting loader on `RuntimeOptions` — single load is proven by `Arc::ptr_eq` between Telegram's and the host's.
- **`--tools-dir` stays optional (user, 2026-11-06):** absent, `list-models` lists the built-ins alone.

- The probe keeps shelling out to `tddy-tools list-models` (one listing code path) and passes the tools dir, rather than the daemon answering curated agents itself.
- Filtering ACP agents out of the PTY sandbox was chosen by the developer (2026-11-06) over keeping them offered.

## Deviations found at the contract

- Constructor call sites: ~117 in 116 files across 4 crates (incl. `tddy-telegram-control` tests), not ~10.
- The CLI listing test lives in `packages/tddy-tools-cli/tests/list_models_tools_dir.rs`, not `tddy-tools/tests/list_models_catalog.rs`: the `tddy-tools` library reads no spec, so only the binary composes registry + listing. `list_models_catalog.rs` holds the flag-parsing guards.
- The choose-agent todo is partly stale: choosing an ACP agent is already refused (by placement, `NotPlaced{Jail}`); only the offered list and the refusal's naming are wrong.
- Green must add `--tools-dir` to the existing probe tests' exact argv vectors (incl. `an_opencode_cli_probe_without_a_resolved_path_omits_the_flag`, passing today).
- Expected cascade conflict in `probe_tests.rs` with #828's 50a79f061 (both edit the probe calls' neighbourhood).
- 4 `probe_tests` fail on this base before any change (see Acceptance Tests).

## TODO

- [x] Record initial discovery
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
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

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code
