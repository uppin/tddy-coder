# Changeset: One host coding-agent registry

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 4 — five crates; `tddy-tools list-models` CLI surface grows a tools dir; ~10 test files re-wired

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

| Test | File |
|---|---|
| `a_daemon_with_an_unparseable_tool_spec_refuses_to_start_naming_the_file` | `packages/tddy-daemon/tests/host_registry_load.rs` |
| `telegram_and_the_session_host_share_one_registry` | `packages/tddy-daemon/tests/host_registry_load.rs` |
| `list_models_honours_a_tools_dir_override_of_the_curated_models` | `packages/tddy-tools/tests/list_models_catalog.rs` |
| `the_catalog_probe_lists_the_hosts_override_not_the_builtins` | `packages/tddy-daemon-rpc/src/catalog/agent_models/probe_tests.rs` |
| `the_pty_sandbox_offers_only_agents_with_a_pty_run` | `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs` |
| `choosing_an_acp_only_agent_in_the_pty_sandbox_is_refused_naming_the_runnable_ones` | `packages/tddy-sandbox-app/tests/agent_from_spec_acceptance.rs` |

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

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — no test needs another leftovers node's behaviour
**Concurrent with:** `feature/agent-tools/session-pointer`, `feature/agent-tools/bounded-waits`, `feature/agent-tools/dead-launch-code`
**Blocks:** `feature/agent-tools/agent-tool-socket-per-user` (threads its socket registry beside a non-optional `coding_agents`)

## Decisions & Trade-offs

- The probe keeps shelling out to `tddy-tools list-models` (one listing code path) and passes the tools dir, rather than the daemon answering curated agents itself.
- Filtering ACP agents out of the PTY sandbox was chosen by the developer (2026-11-06) over keeping them offered.

## TODO

- [x] Record initial discovery
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
