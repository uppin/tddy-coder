# Changeset: Telegram starts agent sessions through the spec launch, with agents and models from the registry

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature (Telegram sessions gain hooks, routes, tmux and activity status)
**Complexity**: 5 — re-scored at red from 4: five crates carry surface (`tddy-daemon-extensions` port, `tddy-agent-launch` os-user start entry, `tddy-session-lifecycle` host starter, `tddy-telegram-control` pickers and handlers, `tddy-daemon` late-bound wiring); a wiring-order change (Telegram is built before the host); a new `/start-agent` command and `tsa:` callback; a pending-session → started-session hand-off (the generic start creates its own session); a 64-byte callback contract; ~20 existing Telegram tests re-pointed and four parent helpers deleted. (Session-token minting dropped by the identity decision.)
**Stack:** `#agent-tools 18/19` — branch `feature/agent-tools/telegram-spec-launch`, base `feature/agent-tools/cli-dispatch-collapse` (#823)
**PR:** [#824](https://github.com/uppin/tddy-coder/pull/824) (draft)
**PRD:** none of its own. Serves the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`): every launcher reads the registry

## Initial Discovery

[2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md](./2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md) — Exploration 2 is this node's own discovery.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-03-cursor-hand-built-launch-paths.md](../todo/2026-11-03-cursor-hand-built-launch-paths.md) | ✅ **RESOLVED HERE** (the remainder): #823 narrows the record to `install_cursor_hooks_in_worktree`, `mint_cursor_chat_id`, `CliSessionManager::start` and `start_cursor`, whose only callers are Telegram's; green deletes all four with Telegram's launcher (`mint_cursor_chat_id` survives only as the `cursor-create-chat` adapter's body if that adapter still wraps it) and deletes the record — a planned deletion (orchestrator, 2026-11-05) |
| [2026-10-22-telegram-tool-sessions-choose-no-provider-account.md](../todo/2026-10-22-telegram-tool-sessions-choose-no-provider-account.md) | ⚠ During. A Telegram CLI session now goes through the route-aware start; whether the operator can *choose* an account from Telegram is not added here — re-check at wrap, promote to ✅ only if the record's ask is met |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-pickers.md`, `oversized-file-telegram-session-control-callbacks.md`, `complexity-telegram-session-control-handle-telegram-branch-callback.md`, `crap-telegram-bot-handlers.md` | ⚠ During. The claude/cursor branches of these shrink; nothing is split here (sweep node). Re-measure at wrap |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-chaining-and-listing.md` | — Reference. Its delete wiring is `feature/agent-tools/agent-config-removal` (n19) |

## Affected Packages

- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): `cli_spawns.rs` launches through a session starter; `pickers.rs`/`callbacks.rs` read the registry and spec models
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): `runtime/telegram_wiring.rs` and `mount_host.rs` hand Telegram a late-bound session starter and the host's coding agents
- [ ] [`tddy-daemon-extensions`](../../../packages/tddy-daemon-extensions/README.md): the `SessionStarter` port, its `NoSessionStarter` null object and the `LateBoundSessionStarter` slot
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md) / [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): `HostSessionStarter` over the os-user-keyed `LaunchSessions::start_session_for_os_user`
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): hand-built `start`/`start_cursor` and their argv builders, once Telegram was their last caller

## Responsibility

- **Telegram's spawn goes through the spec launch.** `spawn_telegram_claude_cli` and `spawn_telegram_cursor_cli` (`cli_spawns.rs:41,287`), a third hand-built claude/cursor launcher, are replaced by one `spawn_telegram_cli_session(session_type, …)` that builds a `StartSessionRequest` and calls the generic start (`LaunchSessions::start_session_core`) through a **session-starter port** set after the host is built. The session therefore gets the spec's hooks, hook token, MCP, provider route, tmux, git identity and metadata — and the start's own branch-conflict guard, replacing Telegram's claude-only check (`pickers.rs:336`).
- **Identity:** the start is keyed by the OS user Telegram already runs its sessions as (`TelegramWorkflowSpawn::os_user`) through an os-user-keyed entry the daemon holds (`LaunchSessions::start_session_for_os_user`); no session token is minted and none reaches the agent (see **Decisions**).
- **Agent picker from the registry:** `/start-agent <prompt>` offers the `CodingAgentRegistry` agents with a PTY transport (`for_session_type`), by id, encoded as an index into that list (`tsa:<agent_idx>|s:<uuid>`) so `callback_data` stays under 64 bytes; the pick records the agent's session type on the pending session and continues from the project keyboard. `/start-claude` and `/start-cursor` stay as shortcuts that pre-pick their agent.
- **Model picker from the spec:** `models.default` + `models.curated` of the chosen agent's spec (default first, labelled by id — no spec declares labels) replace `CLAUDE_CLI_MODELS` / `CURSOR_CLI_MODELS` (`callbacks.rs:83,88`); an agent with `RequiresModel` and no curated models is refused with a message naming it. The drift between Telegram's 5 cursor models and `cursor.yaml`'s 3 disappears.
- **Gates become capabilities or registry lookups**: `is_claude_cli`/`is_cursor_cli` (`pickers.rs:254-255,355,360`), the claude-only conflict path (:569) and the per-agent branch-name helpers (`workflow_spawn.rs:458,468`) read the spec (`BranchNameRequired`), not an agent name.
- No `config.claude_cli` / `config.cursor_cli` read remains in `tddy-telegram-control` (`cli_spawns.rs:118-124`, `resolve_cursor_binary_path` :357).
- Delete Telegram's hand-built launch and the helpers it alone kept alive (see **Removed**).

## Boundaries

- `/start-workflow` (tddy-coder through `TelegramWorkflowSpawn`) is untouched.
- The Telegram delete's worktree removal (`chaining_and_listing.rs:412-420`) is `feature/agent-tools/agent-config-removal` (n19), although the registry handed to Telegram here is what it will use.
- No new Telegram command shape unless the developer asks: `/start-claude` and `/start-cursor` keep working (as shortcuts naming an agent); a generic `/start-agent` is an open option.
- No proto or web change. No file split (code-issue records only).

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable (the generic launch renders the spec).
- No fallback: no `"claude"` binary default (`cli_spawns.rs:123`); a host without coding agents refuses the Telegram start with `FailedPrecondition` surfaced to the chat; a session type the registry does not know is refused, never sent to the `allowed_agents` keyboard; routing-snapshot read errors are reported, not swallowed (`cli_spawns.rs:54,300`; `pickers.rs:253,560`).
- No hangs: the worktree setup and the cursor chat mint go through the generic start's deadlines (`spawn_worker_request_timeout`, #820's mint deadline); the starter port refuses if it was never set rather than waiting.
- Fixtures install coding agents (builtin registry + stub via the spec's `binary.env`) and the sandbox RPC bridge.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n17` cli-dispatch-collapse (#823) | one CLI launch path for every agent; no `CliLauncher`; `FailedPrecondition` without coding agents | Telegram's starter calls it for claude, cursor and any CLI agent | add launch features or re-introduce an agent-id branch |
| `n14` cli-spec-features (#820) | hooks, tmux, conversation id, the cursor chat-mint deadline on the generic path | inherited through the start | change them |
| `n12` agent-registry (#801) | `CodingAgentRegistry`, spec `models`, `Capability::{RequiresModel, BranchNameRequired}` | pickers and gates | add capabilities |

**Parent-green needs found at red (6e/6f).** Each test fails today for this node's reason (a `TODO(telegram-spec-launch)` body or Telegram's own unchanged behaviour), but these cannot pass until the parent is green too — the real objects are kept because the integration *is* the point:

| Test | Needs green |
|---|---|
| `a_telegram_claude_start_launches_the_argv_…`, `a_telegram_start_onto_a_branch_another_session_holds_…` | #823 (claude through the one generic start) and #820 (hooks file + hook token on the generic path, no `TDDY_*` in the agent env) |
| `a_telegram_cursor_start_launches_from_the_cursor_spec_with_a_minted_chat_id` | #823 and #820 (`cursor-create-chat` mint, `.cursor/hooks.json`) |
| `a_telegram_start_of_an_agent_dropped_into_tddyhome_tools_launches_it` | #823 (any registry CLI agent through `start_cli_harness_session`) |
| `the_telegram_agent_picker_…`, `a_telegram_start_on_a_host_without_coding_agents_…`, every unit test, `every_registry_backed_picker_callback_fits_in_64_bytes` | nothing beyond this node |

Inherited red at the base (not this node's): `tddy-daemon` `provider_route_listing_acceptance::list_provider_routes_for_a_dropped_in_agent_offers_the_accounts_its_spec_providers_serve` (n13 route-support's red test); suites using `test_util::builtin_coding_agents` may fail "tool tddy-tools is unavailable" on this n12 head (pre-fixture-fix).

## Draft PR contract

Published by the contract commit (unimplemented bodies are `// TODO(telegram-spec-launch): implement`):

- `tddy-daemon-extensions/src/session_starter.rs`: `OperatorSessionStart`, `OperatorBranchChoice`, `OperatorStartOutcome`, `OperatorBranchConflict`, trait `SessionStarter::start_for_operator`, `NoSessionStarter` (implemented: the null object refuses `FAILED_PRECONDITION`), `LateBoundSessionStarter::{new, bind}` + its `SessionStarter` impl (TODO).
- `tddy-agent-launch/src/operator_start.rs`: `LaunchSessions::start_session_for_os_user(os_user, req, progress)` (TODO).
- `tddy-session-lifecycle/src/connection_service/operator_session_starter.rs`: `HostSessionStarter::new` + its `SessionStarter` impl, `operator_start_request`, `operator_start_outcome` (TODO).
- `tddy-telegram-control/src/telegram_session_control/agent_choices.rs`: `START_AGENT_CMD`, `CB_TELEGRAM_START_AGENT` (`tsa:`), `CB_TELEGRAM_MODEL` (`tcm:`), `TelegramAgentChoice(s)::{of, choices, at}`, `TelegramModelChoice`, `telegram_model_choices`, `telegram_model_at`, `telegram_new_branch_name`, `telegram_agent_pick_callback` / `parse_…`, `telegram_model_pick_callback` / `parse_…`, `parse_start_agent_prompt`, `TelegramPickRefusal` (TODO bodies; `choices` is a getter).
- `TelegramSessionControlHarness::{with_coding_agents, with_session_starter}` (implemented builders; defaults `CodingAgentRegistry::empty()` / `NoSessionStarter`), `handle_start_agent`, `handle_telegram_agent_pick_callback`, `handle_telegram_model_pick_callback` (TODO); `StartAgentCommand`, `StartAgentOutcome`.
- `tddy-daemon`: `Registries::session_starter` (a `LateBoundSessionStarter`), handed to Telegram in `telegram_wiring.rs`; the bind in `install_session_host` and Telegram's `with_coding_agents` are `TODO(telegram-spec-launch)` markers (binding now would hit the TODO body at daemon start).

**Planned deletions at green** (all in § Removed): `spawn_telegram_claude_cli`, `spawn_telegram_cursor_cli`, `handle_telegram_claude_model_callback`, `handle_telegram_cursor_model_callback`, `exposure_is_unwired`, `not_exposed` (+ their 2 unit tests), `CLAUDE_CLI_MODELS`, `CURSOR_CLI_MODELS`, `CB_TELEGRAM_CLAUDE_MODEL`/`CB_TELEGRAM_CURSOR_MODEL` and their parsers (`tcm:` lives on as the generic `CB_TELEGRAM_MODEL`), `send_claude_model_pick_keyboard`, `send_cursor_model_pick_keyboard`, Telegram's claude-only branch-conflict pre-check, `claude_cli_branch_name_from_changeset`, `cursor_cli_branch_name_from_session_id`, the four helpers n17 kept (`install_cursor_hooks_in_worktree`, `mint_cursor_chat_id`, `CliSessionManager::start`, `start_cursor`), the argv builders only they use, and `docs/dev/todo/2026-11-03-cursor-hand-built-launch-paths.md`.

## Green wave

**Wave:** 4 of 5
**Greenable independently:** after #823 is green — the Telegram start must reach the single launch path, and the cursor start depends on the generic cursor launch
**Concurrent with:** none
**Blocks:** `feature/agent-tools/agent-config-removal` (n19: no `config.claude_cli` reader may remain in Telegram before the fields are deleted)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `cli_spawns.rs:41 spawn_telegram_claude_cli`: hand-built worktree, `CliSessionManager::start` with `build_claude_argv`, no hooks/MCP/route/tmux, binary from `config.claude_cli` else `"claude"`, metadata and LiveKit exposure by hand.
- `cli_spawns.rs:287 spawn_telegram_cursor_cli`: `install_cursor_hooks_in_worktree`, `resolve_cursor_binary_path`, `mint_cursor_chat_id` (no deadline), `start_cursor`.
- Pickers gate on session-type strings (`pickers.rs:254-255,355,360`); models are core constants (`callbacks.rs:83,88`); agents come from `allowed_agents()`.
- Telegram is built before the host (`build_context.rs`, `mount_host.rs:222,263-264,304`); `launch_sessions` is `pub(crate)` (`handler_state.rs:193`); Telegram holds no registry and no session token.

## State B

- A Telegram-started agent session is indistinguishable from a web-started one: same spec launch, hooks, activity status, route and branch-conflict guard.
- Telegram offers every PTY agent in the registry with its spec's models.
- No agent name and no daemon.yaml agent section is read in `tddy-telegram-control`.

## Delta

### New
- `tddy-daemon-extensions/src/session_starter.rs` (the port), `tddy-agent-launch/src/operator_start.rs` (the os-user entry), `tddy-session-lifecycle/src/connection_service/operator_session_starter.rs` (the host's starter), `tddy-telegram-control/src/telegram_session_control/agent_choices.rs` (registry-backed choices and callbacks)
- `/start-agent <prompt>` and the `tsa:` agent-pick callback
- Tests: `tddy-telegram-control/tests/telegram_start_agent_from_spec.rs`, `telegram_agent_choices_unit.rs`, `telegram_reads_no_agent_config.rs`; `tddy-daemon-extensions` `session_starter` unit tests; `tddy-session-lifecycle/tests/operator_session_starter_unit.rs`
### Modified
- `tddy-telegram-control`: `telegram_session_control/{cli_spawns,pickers,callbacks,session_start,mod}.rs`, `workflow_spawn.rs`, `telegram_bot.rs`, `Cargo.toml` (+ `tddy-coding-agent-specs` if not reached through session-lifecycle)
- `tddy-daemon`: `runtime/telegram_wiring.rs`, `runtime/mount_host.rs`
- `tddy-session-lifecycle`: starter port implementation over `LaunchSessions`
- Tests: `telegram_start_claude_acceptance.rs`, `telegram_start_cursor_acceptance.rs`, `telegram_branch_conflict_acceptance.rs`, `telegram_session_control_integration.rs` (agent keyboards), `callbacks.rs` unit tests — fixtures move off `ClaudeCliConfig`/`CursorCliConfig` literals onto coding agents
- Docs at wrap: `docs/ft/daemon/telegram-session-control.md`, `claude-cli-session.md` (Telegram section), `cursor-cli-session.md`, `session-branch-conflict.md` § Telegram, `packages/tddy-telegram-control/docs/architecture.md`
### Removed
- `spawn_telegram_claude_cli`, `spawn_telegram_cursor_cli`, `handle_telegram_{claude,cursor}_model_callback`, their hand-written metadata and LiveKit exposure code (`exposure_is_unwired`, `not_exposed`) — the session gets LiveKit PTY exposure the way a web session does (see **Decisions**)
- The pending (draft) session directory once its start succeeds: the generic start creates the session under its own id (`requested_session_id` is honoured for `workspace` only), so Telegram removes the draft and re-binds the chat's tracking to the started id
- `CLAUDE_CLI_MODELS`, `CURSOR_CLI_MODELS` (Telegram) and, if no other caller, `tddy_agent_backend::backend::{claude_cli_models, cursor_cli_models}`
- `claude_cli_branch_name_from_changeset`, `cursor_cli_branch_name_from_session_id` (`workflow_spawn.rs:458,468`), `is_claude_cli`/`is_cursor_cli`
- `install_cursor_hooks_in_worktree` (`tddy-agent-launch/src/chat.rs:14`) and its re-export, `CliSessionManager::{start, start_cursor}`, `build_claude_argv`, `with_claude_launch_flags`, `build_cursor_argv` — each only once no caller remains (`start_with_options` stays for the split agent)
- Telegram's own claude-only branch-conflict check (`pickers.rs:336`)

## Testing Strategy

### Acceptance tests

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_start_agent_from_spec.rs`** (renamed from `…_acceptance.rs` per fluent-tests: `acceptance` names no level) (Telegram harness + a host with builtin coding agents and stub binaries via `binary.env`)

- [x] `a_telegram_claude_start_launches_the_argv_the_claude_spec_renders_with_its_hooks_file_and_hook_token` (written, failing)
- [x] `a_telegram_cursor_start_launches_from_the_cursor_spec_with_a_minted_chat_id` (written, failing)
- [x] `a_telegram_start_of_an_agent_dropped_into_tddyhome_tools_launches_it` (written, failing)
- [x] `the_telegram_agent_picker_lists_the_registrys_cli_agents_and_the_model_picker_lists_the_chosen_specs_models` (written, failing)
- [x] `a_telegram_start_on_a_host_without_coding_agents_is_refused_in_the_chat_spawning_nothing` (written, failing)
- [x] `a_telegram_start_onto_a_branch_another_session_holds_reports_the_start_guards_conflict` (written, failing)

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_branch_conflict_acceptance.rs`**

- [x] `every_registry_backed_picker_callback_fits_in_64_bytes` (written, failing)

### Unit / integration tests

- [x] `telegram_agent_choices_unit.rs`: agent and model callbacks round-trip by index; an out-of-range agent or model index is refused; PTY agents only, by id; default-then-curated models labelled by id; no models when none offered or required; a `RequiresModel` agent with no models is refused with its name; `BranchNameRequired` → `feature/<slug>`, otherwise the start generates the name; `/start-agent` parses.
- [x] Source check `telegram_reads_no_agent_config.rs`: no `claude_cli`/`cursor_cli` read, no per-agent model list, no agent-name gate in `tddy-telegram-control/src` — a scan of this crate of its own (n12's `no_agent_name_gates.rs` is n12's file and is not edited).
- [x] `tddy-daemon-extensions` `session_starter` tests: refused `FAILED_PRECONDITION` before the bind; forwarded unchanged after it; a second bind refused, the first kept.
- [x] `tddy-session-lifecycle/tests/operator_session_starter_unit.rs`: request mapping (new-branch and work-on-branch, `on_branch_conflict = "reject"`, no token); started and branch-conflict outcomes.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Telegram start | Integration with the real launch and stub binaries | the gain is parity with the web start |
| Callback encoding | Unit | the 64-byte contract |

## Decisions & Trade-offs

- **Settled (orchestrator, 2026-11-05) — identity:** the Telegram-initiated session is keyed by an **os-user-keyed entry** the daemon holds (`LaunchSessions::start_session_for_os_user`, reached through `OperatorSessionStart::os_user`). No session token is minted and none goes into the agent's environment. Consequence: a start without a token cannot open a vault account, which is Telegram's behaviour today (`notify_account_identity_unavailable`).
- **Settled (orchestrator, 2026-11-05) — starter placement:** a `SessionStarter` trait in `tddy-daemon-extensions`, bound late through `LateBoundSessionStarter` after the host is built. Telegram is built before the host (`mount_host.rs:222,263,304`), so it gets the port, not the host. The port is proto-free (`OperatorSessionStart`), keeping the extensions crate light.
- **Settled (orchestrator, 2026-11-05) — LiveKit exposure:** Telegram sessions **keep LiveKit PTY exposure** by starting through the same generic start web sessions use; Telegram's own hand-written `expose` goes.
- **Settled (orchestrator, 2026-11-05) — model labels:** the model picker shows the spec's model labels if the spec declares them, else the model ids. No spec declares labels today (`ModelsSection` has ids only, and it is n12's type), so buttons show ids. Agents come from the registry, not `CLAUDE_CLI_MODELS`.
- **Settled (orchestrator, 2026-11-05) — planned deletion:** the four Cursor/claude helpers n17 kept for Telegram and the narrowed todo record go at green (§ Prerequisites, § Draft PR contract).
- **Settled at red (this node) — `/start-agent`:** a registry agent that no shortcut names (an agent dropped into `<tddyhome>/tools`) needs a way to be picked, so `/start-agent <prompt>` shows the agent keyboard first; `/start-claude` / `/start-cursor` stay as pre-picked shortcuts.
- **Settled at red (this node) — pending → started session:** the generic start refuses a caller-chosen id for a CLI session, so the started session has its own id; Telegram removes its pending directory after a successful start and re-binds the chat's tracking. On a refusal or branch conflict the pending session stays, so the operator can choose again.
- **Settled at red (this node) — a host without coding agents:** Telegram holds the host's registry, so an empty one refuses `/start-claude` / `/start-cursor` / `/start-agent` before anything is created ("this host has no coding agents installed: Telegram cannot start a claude-cli session"); the generic start's own `FAILED_PRECONDITION` stays the backstop.
- `write_initial_changeset` in the generic start replaces Telegram's raw `session_type`/`model` markers; the markers become request fields.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the user asked the stack to finish without stopping, 2026-11-05)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
