# 2026-11-07 — One coding-agent registry per host, no empty fallback

**Type:** Refactor

`#agent-tools` 22/28 ([#830](https://github.com/uppin/tddy-coder/pull/830)), branch
`feature/agent-tools/host-registry`, based on `feature/agent-tools/session-pointer` (#829; branch-line
parent only, no surface consumed). Blocks `feature/agent-tools/agent-tool-socket-per-user`, which
threads its socket registry beside the non-optional `coding_agents`.

Packages: `tddy-daemon`, `tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-tools`, `tddy-tools-cli`,
`tddy-sandbox-app`; `tddy-telegram-control` only in its tests (constructor call sites).

## What changed

- **One load.** `tddy-daemon`'s `runtime::build` loads `host_coding_agents(tddy_data_dir)` once,
  before `BuildContext::new`, and fails the start on an error — whatever is mounted, so a daemon
  with no sign-in also refuses a broken `tools/*.yaml`. Telegram (`build_telegram`), the reserved-ids
  step (`mount_host`) and `build_session_host` share the one `Arc` (three loads before). Telegram's
  log-and-empty `telegram_coding_agents` is deleted. `RuntimeTasks::telegram_coding_agents()` exposes
  Telegram's instance; the sharing is proven with `Arc::ptr_eq`, not a counting loader.
- **Non-optional host field.** `DaemonSessionHost::new(…, cli_manager, coding_agents)`;
  `coding_agents: Arc<HostCodingAgents>`; `coding_agents()` returns `&Arc<…>`;
  `with_coding_agents_if_none` and the three `None` → empty fallbacks (`daemon_rpc_handler`,
  `svc_resolve_os_user`, `svc_activity_ports`) are deleted. A host with no agents is built over
  `no_coding_agents()`. ~117 constructor call sites in 116 files moved their chained builder into
  `new`.
- **Model probe.** `list_models_probe_args(agent, binary, tools_dir)` always passes
  `--tools-dir <tddyhome>/tools`; `tddy-tools list-models --tools-dir` lists from
  `CodingAgentRegistry::load(dir)` and refuses a directory that does not load. Without the flag the
  built-ins are listed.
- **PTY sandbox.** `tddy-sandbox-app`'s `choose_agent_on` offers coding agents whose `run` is
  `Transport::Pty`; an ACP-only agent is refused `AgentChoiceError::NotPtyRunnable { id, runnable }`
  (developer's choice over keeping them offered, and over reusing `Unknown`).

## Decisions

- `launch_coding_agents()` returns an owned `Arc` clone, not the `&Arc` the contract text named: a
  reference forced a clone at three call sites for no gain.
- `--tools-dir` stays optional.
- The probe keeps shelling out to `tddy-tools list-models` (one listing code path).

## Resolved backlog

- `docs/dev/todo/2026-11-06-telegram-loads-the-coding-agent-registry-a-third-time.md`
- `docs/dev/todo/2026-11-04-choose-agent-offers-acp-agents-to-pty-sandbox.md`
- `docs/dev/todo/2026-11-02-svc-host-builders-over-length-budget.md`

## Deferred

[`docs/dev/todo/2026-11-07-coding-agent-launcher-loads-the-registry-a-fourth-time.md`](../todo/2026-11-07-coding-agent-launcher-loads-the-registry-a-fourth-time.md):
`DaemonCodingAgentLauncher::for_daemon` still loads the registry itself (outside the planned three
loads); `HarnessRouteSupport::new` and the `tddy-agent-launch` dispatch functions still take an
`Option`; the owned-`Arc` return of `launch_coding_agents()`.

## Restructuring

`svc_host_builders.rs` was 504 production lines and this PR's constructor parameter grew it to
510. The five room-port builders moved by `tddy-tools restructure` (`move_impl_members` into a new
`svc_room_builders`, then `reparent_module` under `svc_host_builders`, beside `rpc_activity`):
`svc_host_builders.rs` **510 → 465**, `svc_host_builders/svc_room_builders.rs` 51. `check --deep`
clean, the apply's compile gate green, `verify --against HEAD` accounted for every statement.

## Code issues

- **Closed:** `tddy-session-lifecycle` `oversized-file-svc-host-builders` — final measurement **465**
  production lines (budget 500; was 504 at its last row). Its backlog entry
  `docs/dev/todo/2026-11-02-svc-host-builders-over-length-budget.md` is deleted with it.
- **Unchanged, touched:** `oversized-file-connection-service` (554 → 554 lines),
  `oversized-file-test-util` (531 → 531).

## Validation

Scoped to the touched packages, on the tree rebased onto #829's `8d3557f93`: `cargo fmt --check` and
`cargo clippy --all-targets -D warnings` over `tddy-daemon`, `tddy-daemon-rpc`, `tddy-sandbox-app`,
`tddy-session-lifecycle`, `tddy-telegram-control`, `tddy-tools`, `tddy-tools-cli` clean; green:
`tddy-daemon --test host_registry_load` (3), `tddy-tools-cli --test list_models_tools_dir` (3),
`tddy-daemon-rpc --lib` (29), `tddy-sandbox-app --lib` (86) and `--test agent_from_spec_acceptance`
(6), `tddy-tools --test list_models_catalog` (5), `tddy-session-lifecycle --lib` (185),
`--test host_coding_agents` (1) and `--test headless_subagents_from_spec_acceptance` (4).
Validate-changes cleanup: `choose_agent_on` reads each def once through two named predicates.
