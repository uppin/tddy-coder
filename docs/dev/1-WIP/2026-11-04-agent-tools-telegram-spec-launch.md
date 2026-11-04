# Changeset: Telegram starts agent sessions through the spec launch, with agents and models from the registry

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature (Telegram sessions gain hooks, routes, tmux and activity status)
**Complexity**: 4 — re-scored from 3 by Exploration 2: more than one crate (`tddy-telegram-control`, `tddy-daemon` runtime wiring, `tddy-session-lifecycle` visibility, session-token minting from `tddy-github`/`tddy-daemon-auth`); a wiring-order change (Telegram is built before the host); a 64-byte callback contract; ~20 tests re-pointed
**Stack:** `#agent-tools 18/19` — branch `feature/agent-tools/telegram-spec-launch`, base `feature/agent-tools/cli-dispatch-collapse` (#823)
**PRD:** none of its own. Serves the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`): every launcher reads the registry

## Initial Discovery

[2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md](./2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md) — Exploration 2 is this node's own discovery.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-03-cursor-hand-built-launch-paths.md](../todo/2026-11-03-cursor-hand-built-launch-paths.md) | ✅ **RESOLVED HERE** (the remainder): #823 narrows the record to `install_cursor_hooks_in_worktree` and `CliSessionManager::start_cursor`, whose only callers are Telegram's; this node removes them with Telegram's launcher and deletes the record |
| [2026-10-22-telegram-tool-sessions-choose-no-provider-account.md](../todo/2026-10-22-telegram-tool-sessions-choose-no-provider-account.md) | ⚠ During. A Telegram CLI session now goes through the route-aware start; whether the operator can *choose* an account from Telegram is not added here — re-check at wrap, promote to ✅ only if the record's ask is met |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-pickers.md`, `oversized-file-telegram-session-control-callbacks.md`, `complexity-telegram-session-control-handle-telegram-branch-callback.md`, `crap-telegram-bot-handlers.md` | ⚠ During. The claude/cursor branches of these shrink; nothing is split here (sweep node). Re-measure at wrap |
| `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-chaining-and-listing.md` | — Reference. Its delete wiring is `feature/agent-tools/agent-config-removal` (n19) |

## Affected Packages

- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): `cli_spawns.rs` launches through a session starter; `pickers.rs`/`callbacks.rs` read the registry and spec models
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): `runtime/telegram_wiring.rs` and `mount_host.rs` hand Telegram a late-bound session starter and the host's coding agents
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md) / [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): a starter port over `LaunchSessions::start_session_core`
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): hand-built `start`/`start_cursor` and their argv builders, once Telegram was their last caller

## Responsibility

- **Telegram's spawn goes through the spec launch.** `spawn_telegram_claude_cli` and `spawn_telegram_cursor_cli` (`cli_spawns.rs:41,287`), a third hand-built claude/cursor launcher, are replaced by one `spawn_telegram_cli_session(session_type, …)` that builds a `StartSessionRequest` and calls the generic start (`LaunchSessions::start_session_core`) through a **session-starter port** set after the host is built. The session therefore gets the spec's hooks, hook token, MCP, provider route, tmux, git identity and metadata — and the start's own branch-conflict guard, replacing Telegram's claude-only check (`pickers.rs:336`).
- **Identity:** the starter acts as the Telegram chat's linked GitHub user (the session-token resolution `start_session_on_request` performs) — mechanism decided at red (see **Decisions**).
- **Agent picker from the registry:** the CLI agents offered are `CodingAgentRegistry` agents with a PTY transport (`for_session_type`), encoded as an index into the sorted list so `callback_data` stays under 64 bytes (assert added to the cursor keyboard, `pickers.rs:580`).
- **Model picker from the spec:** `models.default` + `models.curated` of the chosen agent's spec replace `CLAUDE_CLI_MODELS` / `CURSOR_CLI_MODELS` (`callbacks.rs:83,88`); an agent with `RequiresModel` and no curated models is refused with a message naming it. The drift between Telegram's 5 cursor models and `cursor.yaml`'s 3 disappears.
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

## Draft PR contract

The first push of this PR publishes: the session-starter port (trait or late-bound slot) and its daemon wiring point, `TelegramSessionControlHarness::with_coding_agents`, the registry-backed picker encoders/decoders, and the failing tests below. Unimplemented bodies are `// TODO(telegram-spec-launch): implement`.

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
- A session-starter port (`tddy-daemon-extensions` trait or `tddy-session-lifecycle` handle; placement decided at red)
- `packages/tddy-telegram-control/tests/telegram_start_agent_from_spec_acceptance.rs`
### Modified
- `tddy-telegram-control`: `telegram_session_control/{cli_spawns,pickers,callbacks,session_start,mod}.rs`, `workflow_spawn.rs`, `telegram_bot.rs`, `Cargo.toml` (+ `tddy-coding-agent-specs` if not reached through session-lifecycle)
- `tddy-daemon`: `runtime/telegram_wiring.rs`, `runtime/mount_host.rs`
- `tddy-session-lifecycle`: starter port implementation over `LaunchSessions`
- Tests: `telegram_start_claude_acceptance.rs`, `telegram_start_cursor_acceptance.rs`, `telegram_branch_conflict_acceptance.rs`, `telegram_session_control_integration.rs` (agent keyboards), `callbacks.rs` unit tests — fixtures move off `ClaudeCliConfig`/`CursorCliConfig` literals onto coding agents
- Docs at wrap: `docs/ft/daemon/telegram-session-control.md`, `claude-cli-session.md` (Telegram section), `cursor-cli-session.md`, `session-branch-conflict.md` § Telegram, `packages/tddy-telegram-control/docs/architecture.md`
### Removed
- `spawn_telegram_claude_cli`, `spawn_telegram_cursor_cli`, their hand-written metadata and LiveKit exposure code (exposure re-provided only if the generic start lacks it — see **Decisions**)
- `CLAUDE_CLI_MODELS`, `CURSOR_CLI_MODELS` (Telegram) and, if no other caller, `tddy_agent_backend::backend::{claude_cli_models, cursor_cli_models}`
- `claude_cli_branch_name_from_changeset`, `cursor_cli_branch_name_from_session_id` (`workflow_spawn.rs:458,468`), `is_claude_cli`/`is_cursor_cli`
- `install_cursor_hooks_in_worktree` (`tddy-agent-launch/src/chat.rs:14`) and its re-export, `CliSessionManager::{start, start_cursor}`, `build_claude_argv`, `with_claude_launch_flags`, `build_cursor_argv` — each only once no caller remains (`start_with_options` stays for the split agent)
- Telegram's own claude-only branch-conflict check (`pickers.rs:336`)

## Testing Strategy

### Acceptance tests

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_start_agent_from_spec_acceptance.rs`** (Telegram harness + a host with builtin coding agents and stub binaries via `binary.env`)

- [ ] `a_telegram_claude_start_launches_the_argv_the_claude_spec_renders_with_its_hooks_file_and_hook_token`
- [ ] `a_telegram_cursor_start_launches_from_the_cursor_spec_with_a_minted_chat_id`
- [ ] `a_telegram_start_of_an_agent_dropped_into_tddyhome_tools_launches_it`
- [ ] `the_telegram_agent_picker_lists_the_registrys_cli_agents_and_the_model_picker_lists_the_chosen_specs_models`
- [ ] `a_telegram_start_on_a_host_without_coding_agents_is_refused_in_the_chat_spawning_nothing`
- [ ] `a_telegram_start_onto_a_branch_another_session_holds_reports_the_start_guards_conflict`

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_branch_conflict_acceptance.rs`**

- [ ] `every_registry_backed_picker_callback_fits_in_64_bytes`

### Unit / integration tests

- [ ] `callbacks.rs`: agent and model callbacks round-trip by index; an out-of-range index is refused.
- [ ] `pickers.rs`: a `RequiresModel` agent with no curated models is refused with its name; an agent without `BranchNameRequired` gets a generated branch name.
- [ ] Source check: no `claude_cli`/`cursor_cli` field read and no agent-name literal gate in `tddy-telegram-control/src` (extend n12's scan to this crate).

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Telegram start | Integration with the real launch and stub binaries | the gain is parity with the web start |
| Callback encoding | Unit | the 64-byte contract |

## Decisions & Trade-offs

- **Open — identity:** mint a session token for the chat's linked GitHub login (`tddy-github` `mint_access`) or add an os-user-keyed start entry to `LaunchSessions`. Proposed: an os-user-keyed entry on the starter port, avoiding a token round-trip inside the daemon.
- **Open — starter placement:** a `SessionStarter` trait in `tddy-daemon-extensions` (Telegram stays a satellite) set after `install_session_host`, versus a `OnceLock` handle in session-lifecycle. Proposed: the trait.
- **Open — LiveKit exposure:** Telegram's claude start exposes the PTY over LiveKit (`cli_spawns.rs:194-239`); the generic start returns empty LiveKit fields (`opencode_cli_spawn.rs:328`). Either the generic start gains exposure (a #820-shaped gap — raise there) or Telegram sessions lose it. Ask before dropping it.
- **Open — model labels:** spec `models.curated` has ids only; Telegram's claude buttons showed labels. Show ids, or add labels to the spec (spec-crate change).
- `write_initial_changeset` in the generic start replaces Telegram's raw `session_type`/`model` markers; the markers become request fields.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-telegram-spec-launch-initial-discovery.md`)
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
