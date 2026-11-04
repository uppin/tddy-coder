# Changeset: Agent settings come only from specs; the daemon.yaml agent types and readers are deleted

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature (breaking configuration change)
**Complexity**: 5 — several row-4 signals: crates `tddy-daemon-kernel`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-activity`, `tddy-session-split`, `tddy-daemon-rpc`, `tddy-telegram-control`, `tddy-daemon`, `tddy-coding-agent-specs`; a breaking config-type removal; an answer-wait hold whose value must match what the agent was launched with (ordering); ~40 fixture files migrated; `coding_agents` becomes non-optional
**Stack:** `#agent-tools 19/19` — branch `feature/agent-tools/agent-config-removal`, base `feature/agent-tools/telegram-spec-launch` (#824)
**PRD:** none of its own. Completes the "daemon.yaml agent sections removed" criterion of the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`)

## Initial Discovery

[2026-11-04-agent-tools-agent-config-removal-initial-discovery.md](./2026-11-04-agent-tools-agent-config-removal-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-removed-agent-config-readers-remain.md](../todo/2026-11-04-removed-agent-config-readers-remain.md) | ✅ **RESOLVED HERE.** Fields, types and readers deleted; `daemon_config_from_fixture_file` deleted; Telegram's delete gets the registry |
| `docs/dev/todo/2026-11-04-telegram-delete-leaves-agent-worktrees.md` | ✅ **RESOLVED HERE** — but the file **does not exist** at #801's head (`0dbba9195`); the `TODO(agent-registry)` at `chaining_and_listing.rs:412-420` names it. If #801 adds it before this node greens, delete it at wrap; otherwise remove the dangling reference with the TODO |
| [2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) | ⚠ During. This node migrates the two `tddy-daemon-rpc` fixtures that use `daemon_config_from_fixture_file`; deleting the agentless `DaemonCodingAgentLauncher` hand-built ACP path stays the sweep node's unless making `coding_agents` non-optional reaches `DaemonCodingAgentLauncher::{new, with_identity}` too — **decide at red** |
| [2026-11-03-harness-clis-legacy-cli-harness-removal.md](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md) | ⚠ During. `cli_harness_binary` and `CliHarnessConfig` go here; `svc_terminal_ports.rs:268` moves off the legacy `CliHarness` + `cli_harness_binary` onto the spec. The `CliHarness` table itself stays (sweep) |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-coding-agent-launcher.md`, `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-chaining-and-listing.md` | ⚠ During. Touched at the binary reads and the delete wiring; no split (sweep). Re-measure at wrap |

## Affected Packages

- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}`, `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig` and their readers deleted; `refuse_removed_agent_keys` kept
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): binary, tddy-tools and hook-url readers move here (spec + host); binary resolvers refuse instead of returning the configured value as-is; `LaunchSessions.coding_agents` non-optional
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `AnswerWaitPorts` holds per agent
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `coding_agent_launcher.rs` binary reads, `svc_activity_ports.rs`, `svc_terminal_ports.rs`; ~34 fixture files; `test_util::daemon_config_from_fixture_file` deleted
- [ ] [`tddy-session-split`](../../../packages/tddy-session-split/README.md): hook url, answer wait, claude binary reads
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): `catalog/ports.rs` model probe; 2 fixtures
- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): the delete passes the registry; test struct literals
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): 4 fixtures; Telegram wiring passes the registry
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `claude.yaml` `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`

## Responsibility

- **Delete** the daemon-kernel `claude_cli`, `cursor_cli`, `cli_harnesses` fields (`config.rs:155,158,162`), the three types (`agent_cli.rs:148-188,248-262`) and their readers (`resolve_claude_binary_path`, `claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`, `resolve_cursor_binary_path`, `resolve_cursor_cli_tddy_tools_path`, `resolve_cursor_cli_daemon_url`, `resolve_cursor_home_dir`, `resolve_claude_home_dir`, `daemon_hook_urls::claude_hook_daemon_url`). `refuse_removed_agent_keys` stays: a daemon.yaml naming them still fails start-up with the replacement named.
- **Readers move up to the launch crates**, because the kernel has no edge to specs: the binary is `HostCodingAgents::resolve(id, …)` (spec `binary.env`, then host auto-resolution in `DaemonToolHost::located` without the config); tddy-tools from `tools/tddy-tools.yaml`/the host; the hook url from `local_daemon_hook_url`/`listen.advertise_url`.
- **Binary resolvers' "as-is" last step becomes a refusal** (`resolve_claude_binary_path` step 4, `cli_harness_binary`'s `default_binary`): a binary not found is refused naming the agent and its spec's `binary.env`.
- **`coding_agent.answer_wait_secs` is wired to its callers**: the hook parameters at launch (`claude_hooks.rs:51,118-120` writes `timeout = wait + margin`) read the spec value, and the **server-side hold is per agent and matches the hook timeout written at launch** — the wait is recorded in the session's metadata at start and `AnswerWaitPorts` reads it per session, never re-reading a since-edited override. A session with no recorded wait uses the spec default (`DEFAULT_ANSWER_WAIT_SECS`, specs), never a daemon-wide value.
- **`claude.yaml` declares `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`**, and the out-of-band env additions are removed (`claude_cli_renderer_env`, `svc_start_sandboxed_claude_cli_session.rs:394`, `svc_relaunch_sandboxed_runner.rs:158` — wherever they survive #823); the jailed process gets it through `run.env`.
- **Fixtures:** the ~40 callers of `daemon_config_from_fixture_file` (session-lifecycle 34, daemon 4, daemon-rpc 2) move onto `test_config()`/`DaemonConfig::load` with a fixture host mapping each spec's `binary.env` to a stub (`test_util::coding_agents_with_binaries`), and the function is deleted. Telegram test struct literals drop the fields.
- **`LaunchSessions.coding_agents` becomes non-optional** (`launch_ports.rs:130`): every launcher has coding agents; the "no coding agents → `FailedPrecondition`" refusals become construction-time guarantees where the type allows.
- **Telegram delete removes an agent session's worktree again**: `chaining_and_listing.rs:412-420` passes the registry #824 gave the controller to `delete_session_directory`, so `worktree_removal_applies_to` follows the spec's `KeepWorktreeOnDelete`.
- **Changelog entry** for the breaking change in `docs/ft/daemon/changelog/` (removed keys, their spec replacements, the binary refusal, `claude_home_dir` with no equivalent, the route-shape renames from #819).

## Boundaries

- Does not touch the `CliHarness` table, the agentless ACP launcher path (unless decided above), or `SupervisorClient::call`'s deadline — sweep node.
- Does not change any spec semantics beyond `claude.yaml`'s `run.env` and, if needed, `answer_wait_secs: 120` declared explicitly.
- No proto or web change. No file splits (code-issue records only).

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable: the readers removed here were the last daemon-config-to-agent env path; a test asserts a claude-cli start's environment has no `TDDY_` key.
- No production fallback without coding agents (now a type guarantee); no "as-is" binary fallback.
- No hangs: the answer hold is bounded by the per-session recorded wait.
- Fixtures install coding agents and the sandbox RPC bridge; Linux jail tests self-skip without user namespaces.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n16` spec-backend (#822) | no workflow backend built from an agent-id match | the kernel config has no backend-side reader left | change `SpecBackend` |
| `n18` telegram-spec-launch (#824) | Telegram launches through the spec path, reads no `claude_cli`/`cursor_cli`, and holds the host's coding agents | the delete uses that handle; the fields can go | change Telegram's start or pickers |
| `n17` cli-dispatch-collapse (#823) | the bespoke claude/cursor launch code (and most of its config reads) deleted | only the remaining readers are moved here | re-add any launch path |
| `n12` agent-registry (#801) | `refuse_removed_agent_keys`, `CodingAgentSpec::answer_wait_secs`, `Capability::KeepWorktreeOnDelete` | kept; wired; used by the delete | change the refusal text |

## Draft PR contract

The first push of this PR publishes: `test_util::coding_agents_with_binaries`, the per-session answer-wait type on `AnswerWaitPorts`, the binary-not-found refusal error, `TelegramSessionControlHarness` delete wiring point, `claude.yaml`'s `run.env` line, and the failing tests below. Unimplemented bodies are `// TODO(agent-config-removal): implement`. No field is deleted in the contract commit.

## Green wave

**Wave:** 5 of 5
**Greenable independently:** after #822 and #824 are green — the fields can be deleted only once no backend (#822) and no Telegram path (#824) reads them
**Concurrent with:** none
**Blocks:** nothing in this stack (the reserved sweep node, n20, follows)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}` still exist (`config.rs:155-162`); `DaemonConfig::load` refuses them, but `serde_yaml::from_str` and struct literals still set them.
- 28 production read sites in 19 files outside the kernel (discovery §1.3), fewer after #823 and #824.
- `answer_wait_secs` comes from `claude_cli.answer_wait_secs`; the server hold is one daemon-wide `Duration` (`svc_activity_ports.rs:103` → `question_answers.rs:47`).
- `claude.yaml` has `run.env: {}` (:29); `CLAUDE_CODE_NO_FLICKER` is added by `claude_cli_renderer_env` outside the spec.
- `daemon_config_from_fixture_file` (`test_util.rs:58-70`) has 40 caller files.
- Telegram's delete passes `None` (`chaining_and_listing.rs:412-420`), leaving every agent worktree behind.

## State B

- No agent setting is read from daemon.yaml anywhere; the kernel has no agent type.
- Each agent's answer hold equals the hook timeout it was launched with.
- A claude-cli session's no-flicker env comes from `claude.yaml` and an override can drop it.
- A Telegram delete removes an agent session's worktree unless its spec keeps it.

## Delta

### New
- `docs/ft/daemon/changelog/2026-11-04-agent-tools-daemon-yaml-agent-sections-removed.md` (at wrap)
- `packages/tddy-session-lifecycle/src/test_util.rs` `coding_agents_with_binaries`
### Modified
- `tddy-daemon-kernel`: `config.rs`, `config/agent_cli.rs` (keepers only: `find_real_harness_on_host`, `resolve_harness_binary_name`, `choose_*_binary`), `config/daemon_hook_urls.rs`
- `tddy-agent-launch`: `host_coding_agents.rs`, `hooks_and_urls.rs`, `notify_env.rs`, `harness_from_spec.rs` (`configured_cli_binary`), `cli_harness_jail.rs`, `launch_ports.rs`, plus any reader #823 left
- `tddy-session-activity/src/service/question_answers.rs`; `tddy-session-lifecycle/src/connection_service/{svc_activity_ports,svc_terminal_ports}.rs`, `coding_agent_launcher.rs`
- `tddy-session-split/src/{svc_spawn_split_agent,service_util}.rs`; `tddy-daemon-rpc/src/catalog/ports.rs`
- `tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs`; `tddy-daemon/src/runtime/telegram_wiring.rs`
- `tddy-coding-agent-specs/specs/claude.yaml`
- Fixtures: the 40 files in discovery §1.6; `tddy-telegram-control/tests/{telegram_branch_conflict_acceptance,telegram_start_claude_acceptance,telegram_start_cursor_acceptance}.rs` literals (if #824 left any); `claude_cli_fullscreen_renderer.rs:176`, `unified_worktree_base_acceptance.rs:73`
- Docs at wrap: `docs/ft/daemon/{claude,cursor,opencode,muse,warp}-cli-session.md`, `session-agent-roster.md`, `telegram-session-control.md`, `packages/tddy-daemon-kernel/README.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (`run.env`, `answer_wait_secs`), `config.example.yaml`, `dev.daemon.yaml:130`
### Removed
- `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}` and their `Default` entries and re-exports (`config.rs:313-318`)
- `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig`
- `resolve_claude_binary_path`, `claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`, `resolve_cursor_binary_path`, `resolve_cursor_cli_tddy_tools_path`, `resolve_cursor_cli_daemon_url`, `resolve_cursor_home_dir`, `resolve_claude_home_dir`, `daemon_hook_urls::claude_hook_daemon_url`, `CLAUDE_FULLSCREEN_ENV` once unused
- `configured_binary` (`spec_launch.rs:475`, if #823 left it), `configured_cli_binary` and its `TODO(agent-registry)`, `configured_cursor_binary`
- The kernel's `DEFAULT_ANSWER_WAIT_SECS` (the specs' stays)
- `tddy_session_lifecycle::test_util::daemon_config_from_fixture_file` and its `TODO(agent-registry)`
- Kernel tests `config.rs:993 claude_cli_config_tests`, `:1218 cursor_cli_config_tests`, `tests/answer_wait_secs_config.rs`
- The `TODO(agent-registry)` at `chaining_and_listing.rs:412-420`
- The `Option` around `LaunchSessions.coding_agents`

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`** (`agent_registry_support`, `a_hook_reports_running` :330)

- [ ] `an_override_of_claude_yaml_with_answer_wait_secs_holds_a_permission_hook_that_long_and_writes_that_timeout_into_the_hooks_file`
- [ ] `editing_answer_wait_secs_after_a_start_does_not_change_that_sessions_hold`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_fullscreen_renderer.rs`** (rewritten off `.claude_cli` :176)

- [ ] `a_claude_cli_start_has_claude_code_no_flicker_from_claude_yaml_and_an_override_without_it_does_not`
- [ ] `jailed::a_jailed_claude_cli_start_has_claude_code_no_flicker_from_claude_yaml` (macOS only)

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_session_acceptance.rs`**

- [ ] `a_claude_cli_start_whose_binary_is_not_found_is_refused_naming_its_binary_env`

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_session_control_integration.rs`**

- [ ] `a_telegram_delete_of_a_claude_cli_session_removes_its_worktree_and_of_a_cursor_cli_session_keeps_it`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs`** (sibling scan)

- [ ] `the_fixtures_start_every_agent_from_a_daemon_yaml_with_no_agent_section` (no `daemon_config_from_fixture_file`, no `claude_cli:`/`cursor_cli:`/`cli_harnesses:` in test sources)

Kept green: `tddy-daemon/tests/removed_agent_config_acceptance.rs`, `tddy-daemon-kernel/tests/removed_agent_keys_unit.rs`.

### Unit / integration tests

- [ ] `tddy-session-activity`: the hold reads the session's recorded wait; an unrecorded session uses the spec default.
- [ ] `tddy-agent-launch`: `DaemonToolHost::located` finds `claude` with no config; a missing binary is refused naming `binary.env`.
- [ ] `tddy-daemon-kernel`: a `DaemonConfig` without agent sections round-trips; the type has no agent field (compile-level).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Answer hold | Integration with a hook report | the hold/timeout pairing is the hang risk |
| No-flicker env | Integration (host and jail) | the env previously reached the jail out of band |
| Fixture migration | Source scan | keeps the property |

## Decisions & Trade-offs

- **Breaking removal**, already refused at load by #801; this deletes what was left behind. Changelog entry required.
- **Open — `claude_home_dir`:** the removed knob was an absolute directory; `home: { kind: persistent, key }` is a name under `<tddyhome>/homes`. Operators with a custom directory have no equivalent. Accept the loss (changelog) or add a path form to `HomeKind` — ask.
- **Open — hook url:** `cursor_cli.daemon_url` fell back cursor → claude → port; `listen.advertise_url` replaces both. Confirm no deployment relied on distinct hook urls per agent.
- Complexity 5: consider greening in this order — readers move up, answer wait, `run.env`, fixtures, then the deletion commit.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-agent-config-removal-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
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
