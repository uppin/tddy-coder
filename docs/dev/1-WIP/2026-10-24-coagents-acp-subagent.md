# Changeset: A coding agent as a session subagent, over ACP

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — proto + wire-record change, a new crate, a spawned-as-user process seam, four crates and the web, managed confinement per harness is partly unsettled (see Decisions)
**Branch**: `feature/coagents/acp-subagent`, base `feature/coagents/acp-driver` (#720) — `#coagents` 2/8
**PR**: [#721](https://github.com/uppin/tddy-coder/pull/721)

## Initial Discovery

[2026-10-24-coagents-acp-subagent-initial-discovery.md](./2026-10-24-coagents-acp-subagent-initial-discovery.md)

## Prerequisites

No code issue in a touched package carries `Claimed by:`.

### ⚠ DURING — Drain `tddy-discovery` into purpose-named crates — [`2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md`](../todo/2026-10-08-drain-tddy-discovery-into-purpose-named-crates-and-remove-it.md)

New code does **not** go into `tddy-discovery`. The coding-agent session lives in a new
purpose-named crate, `tddy-coding-subagent`, which depends on `tddy-discovery` only for the
`SubagentSession` trait and outcome types (they move to `tddy-subagent` when the drain runs; this
crate follows them).

### ⚠ DURING — A conversation id is not bound to the session that opened it — [`2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md`](../todo/2026-09-26-a-conversation-id-is-not-bound-to-the-session-that-opened-it.md)

A coding-agent conversation lives in the same host-global map. No worse than a `model` conversation;
the stake is higher because the agent can write. Recorded, not fixed here.

### ⚠ DURING — A subagent turn has no wall-clock deadline — [`2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md`](../todo/2026-09-27-a-subagent-turn-has-no-wall-clock-deadline.md)

A coding agent stalls the same way; `subagent_cancel` (now real `session/cancel`) is the way out.

### ℹ ANSWERED — Daemon-run conversations still write the session worktree — [`2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md`](../todo/2026-09-30-daemon-run-subagent-conversations-still-write-the-session-worktree.md)

For this node that is the **intended** default ("same worktree as the main agent"). n7
`clone-placement` claims the entry.

### ℹ ANSWERED — Tool-session provider picker needs harness metadata — [`2026-10-22-tool-session-provider-picker-needs-harness-metadata.md`](../todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md)

`SubagentInfo.harness` is that metadata for the subagent catalog; `AgentInfo` (the tool-session
picker) is not changed here.

## Responsibility

- **Kind on the wire and at rest.** A shared `AgentKind { MODEL = 0, CODING_AGENT = 1 }` and an
  optional `harness` on `SubagentInfo` and `SessionAgentEntry`; `kind` + `harness` on
  `tddy_changeset::SessionAgentRecord` (serde default `Model`, `harness: None`).
- **The coding-agent catalog.** `CodingAgentDef { name, label, harness, model, provider_account }`
  resolved from ACP harness profiles and ACP `allowed_agents` ids; rows added to `ListSubagents`
  after the `model` rows.
- **Admission.** Attaching a `coding_agent` row; refusing, for a managed session, a harness whose
  native tools cannot be withdrawn (`HarnessConfinement::cannot_withdraw`).
- **The session.** `tddy_coding_subagent::CodingAgentSession: SubagentSession`, driving n1's
  `AcpDriver`: turn → `PromptOutcome`, ACP tool calls → `MessageDescriptor`s, loop-only request
  options refused (`SubagentError::UnsupportedByCodingAgent { option }`), permission requests denied
  through a `DenyAndRecord` `PermissionHandler`.
- **The launch.** `CodingAgentLauncher` — the port `tddy-session-agents` calls to start the agent
  process for a conversation: as the session's OS user, the provider account's route applied, cwd on
  the session worktree; in a managed session native tools withdrawn and `tddy-tools --mcp` handed to
  the agent as an MCP server pointed at the session's exec tools. Its daemon adapter lives in
  `tddy-session-lifecycle`.
- **Open / turn / end.** `open_local_agent_session` builds a `CodingAgentSession` for a
  `coding_agent` record; cancel, end and detach stop the process.
- **Web.** The kind badge in `CreateSessionAgentPickerSection`, `AgentPicker` and `RosterAgentRow`.

## Boundaries

- No permission yield to the main agent — n3 `permission-yield` replaces `DenyAndRecord`.
- No transcript persistence, no activity bubble — n5 `conversation-bubble`.
- No separate clone placement — n7 `clone-placement`. Conversations run on the session worktree.
- No remote coding agents (`name@other-daemon` with `kind: coding_agent` is refused at attach) — n8 `remote-codebase`.
- No headless `-p` harnesses (`claude`, `cursor`, `codex`) — n4 `headless-driver`; `ListSubagents` does not list them yet.
- No `replaces` for coding agents (always empty).
- `model` entries and the in-jail loop are untouched.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| n1 `acp-driver` (#720) | `AcpDriver::{spawn, open_session, prompt, cancel}`, `OpenSession { cwd, resume, model, mcp_servers }`, `AcpTurnEnd`, `AcpUpdate`, `PermissionHandler`, `McpServerSpec` in `tddy-agent-backend` | `CodingAgentSession` holds one `AcpDriver` per conversation; installs `DenyAndRecord`; maps `AcpUpdate`s to descriptors | change the driver, add a transport, or touch the three workflow backends |

## Draft PR contract

The first push after this plan publishes:

- Proto fields (`AgentKind`, `harness`) with regenerated Rust/TS.
- `SessionAgentRecord.kind` / `.harness`.
- `packages/tddy-coding-subagent` crate with `CodingAgentDef`, `CodingAgentSession`, `CodingAgentLauncher`,
  `HarnessConfinement`, `DenyAndRecord` — bodies `// TODO(acp-subagent): implement`.
- `SubagentError::UnsupportedByCodingAgent`.
- The failing tests below.

Dependents compile against `AgentKind`, `CodingAgentSession`, `CodingAgentLauncher`, `CodingAgentDef`.

## Green wave

**Wave:** 2 of 4
**Greenable independently:** no — needs n1's `AcpDriver` *behaviour* (its tests drive a real stub turn through it)
**Concurrent with:** none
**Blocks:** n3 `permission-yield`, n5 `conversation-bubble`, n7 `clone-placement`, n8 `remote-codebase`

    n1 → n2      n2 → n3, n5, n7, n8      n3 → n4, n6      n5 → n6

## Affected Packages

- **tddy-service**: `proto/catalog.proto`, `proto/session_agents.proto` (+ the shared enum) — regenerated in `tddy-web` and `tddy-rust-typescript-tests`
- **tddy-changeset**: `SessionAgentRecord.kind` / `.harness`
- **tddy-coding-subagent** (new): [README.md](../../packages/tddy-coding-subagent/README.md) — the session, catalog, launcher port, confinement table
- **tddy-discovery**: `SubagentError::UnsupportedByCodingAgent` only
- **tddy-session-agents**: `open_local_agent_session` branch on kind; attach admission; catalog rows
- **tddy-session-lifecycle**: `CodingAgentLauncher` adapter (spawn as the session's OS user, route, MCP config)
- **tddy-daemon-rpc**: `ListSubagents` rows
- **tddy-supervisor**: `SpawnRelayed` (stdio relayed over a daemon-owned socket), its own allowlist `allowed_relay_tool_paths` — see *Privileged-surface change*
- **tddy-agent-backend**: `AcpDriver::spawn_over` (serve an agent over a stream; additive), `codex_acp_route_args` exported
- **tddy-session-split**: `coding_agent_wiring` (the managed agent's `tddy-tools --mcp` server and credential)
- **tddy-web**: kind badge in two pickers and the Agents tab

## Related Feature Documentation

- [tddy-supervisor.md](../../ft/supervisor/tddy-supervisor.md) § Relayed spawns

- [PRD-2026-10-24-coagents-acp-subagent.md](../../ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md)
- [session-agent-roster.md](../../ft/daemon/session-agent-roster.md), [harness-profiles.md](../../ft/daemon/harness-profiles.md), [managed-codebase-subagents.md](../../ft/coder/managed-codebase-subagents.md)

## Summary

A roster entry can be an ACP coding agent. The daemon starts it as the session's user on the
session's worktree, under the main agent's confinement, and the existing `subagent_*` tools talk to it.

## Background

See the PRD.

## Scope

- [ ] **Implementation**: kind on the wire and at rest; catalog rows; admission; `CodingAgentSession`; launcher adapter; web badge
- [ ] **Testing**: acceptance tests below
- [ ] **Package Documentation**: `tddy-coding-subagent` README; roster doc § kinds
- [ ] **Code Quality**: no growth in over-budget files beyond wiring

## Technical Changes

### State A (Current)

- `tddy-tools/src/server.rs:1763-1853` `subagent_new_session`: `roster.local_def_for(&entry)` (seed def present) → in-process `SpecializedSubagentSession`; else `open_remote_agent_session` → daemon `OpenAgentConversation`.
- `tddy-session-agents/src/service.rs:783-884` `OpenAgentConversation` → `open_owned` → `open_local` (record on this daemon) → `peers.open`.
- `svc_start_hosted_agent_clone.rs:153-191` `open_local_agent_session` always builds `SpecializedSubagentSession` from `agent_def_for_spawn`; `local_agent_codebase_access` (`:269-334`) runs exec tools on the session worktree as the daemon's identity.
- `service.rs:551-606` attach: `catalog.record_for` → `admission.admit` → `rosters.attach`; `roster_record_for` resolves only `resolvable_agent_defs` (`svc_turn_end_reporter.rs:52-74`).
- `tddy-daemon-rpc/src/catalog/ports.rs:167-185` `ListSubagents` = `resolvable_agent_defs` only; `agent_def.rs:221-231` skips `harness:` documents; `docs/ft/daemon/harness-profiles.md`: profiles "cannot be attached to a session's roster".
- `catalog.proto:73-91` `SubagentInfo` fields 1-7; `session_agents.proto:309-347` `SessionAgentEntry` 1-12; `SessionAgentRecord` (`tddy-changeset/src/session_agent.rs`) has no kind.
- Coding-agent processes are spawned elsewhere as the OS user via `spawn_as_user` / the spawn worker (`tddy-daemon-kernel/src/spawn_as_user.rs:194-225`, `tddy-agent-launch/src/tool_session_spawn.rs:241-262`); routes are resolved for tool sessions by `tddy-agent-launch` (`harness_profile_request.rs`, `provider-route.json`).
- Web: `useAvailableAgents.ts:22-31` `AvailableAgent` (no kind); `CreateSessionAgentPickerSection.tsx:16-59`; `AgentPicker.tsx:44-51`; `SessionAgentTree.tsx:253-300` `RosterAgentRow`.

### State B (Target)

```
subagent_new_session{agent:"codex@A"}  (no seed def → daemon, unchanged route)
   └─► daemon A OpenAgentConversation → open_local(record kind=coding_agent)
          └─► CodingAgentSession::open(def, launcher, cwd/session worktree, managed?)
                 └─► CodingAgentLauncher (spawn as session OS user, route, MCP: tddy-tools --mcp)
                        └─► AcpDriver (n1) ── ACP stdio ── codex-acp / claude-agent-acp / opencode acp
   subagent_prompt → take_turn → AcpDriver::prompt → PromptOutcome{stopReason, content, usage, messages}
```

### Delta

See *Responsibility*, per package as listed in *Affected Packages*.

## Implementation Milestones

- [x] proto + record kind, back-compat load
- [x] `CodingAgentDef` resolution and `ListSubagents` rows
- [x] attach admission incl. managed confinement refusal
- [x] `CodingAgentSession` turn / descriptors / refusals / deny-and-record / cancel / drop
- [x] `CodingAgentLauncher` daemon adapter — user (privileged daemon), route, managed withdrawal + MCP; supervised-host user switch is a 🔴 TODO
- [x] web badge

## Testing Plan

**Level:** integration for the session (real `tddy-acp-stub` child through n1's driver) and the
daemon RPCs (the `tddy-daemon-rpc` acceptance harness already used for `ListSubagents` and
conversations, with the launcher port injected so the spawn request — user, cwd, MCP config — is
observable without root); Cypress component tests for the badge (`mountWithRpc` + in-memory backend).

## Acceptance Tests

Run with `cargo build -p tddy-acp-stub` first (the session and daemon suites spawn it), then
`cargo test -p tddy-coding-subagent` and `cargo test -p tddy-daemon-rpc --test coding_subagent_acceptance`;
web: `bun run cypress:component --spec cypress/component/CreateSessionAgentPicker.cy.tsx,cypress/component/SessionAgentRosterStatus.cy.tsx`.

`packages/tddy-coding-subagent/tests/coding_agent_session_acceptance.rs` (`#[serial]`, real stub through n1's `AcpDriver`) — fail at `CodingAgentSession::start`'s `TODO(acp-subagent)` (and need #720 green):

1. `a_prompt_turn_returns_the_coding_agents_text_as_end_turn`
2. `each_tool_call_of_the_turn_is_one_message_descriptor` — `Read`, `Edit` descriptors in order
3. `a_loop_only_option_is_refused_naming_it` — `maxTurns`, `yieldConditions`, `fromMessageId`, `correction`, `replacement`
4. `a_permission_request_is_denied_and_recorded_in_the_turns_messages` — stub records `selected:reject-once`; an `is_error` descriptor previews the title
5. `cancelling_a_running_turn_returns_cancelled` — through a `CancelHandle` taken before the turn
6. `dropping_the_session_stops_the_agent_process`

`packages/tddy-coding-subagent/tests/catalog_acceptance.rs`:

7. `an_acp_harness_profile_is_a_coding_agent` (fails: empty catalog)
8. `a_headless_or_pty_harness_profile_is_not_offered_yet` — **passes at contract**: a negative pinned by the empty TODO body; it stays meaningful once 7 is green
9. `an_acp_allowed_agent_is_a_coding_agent_on_its_default_model` (fails)
10. `profiles_come_before_allowed_agents` (fails)
11. `claude_and_opencode_can_withdraw_their_native_tools_and_codex_cannot` (fails on `claude-acp`)

`packages/tddy-daemon-rpc/tests/coding_subagent_acceptance.rs` (new, one file for listing, attaching and conversations; a `RecordingLauncher` injected through `test_service_with_coding_agent_launcher` records each `CodingAgentLaunch` and starts the stub) — fail because no coding-agent row is listed yet:

12. `an_acp_harness_profile_is_listed_as_a_coding_agent_with_its_harness`
13. `an_acp_allowed_agent_is_listed_as_a_coding_agent_with_its_default_model`
14. `a_headless_harness_profile_is_not_listed_yet` — **passes at contract** (negative)
15. `attaching_a_coding_agent_streams_a_coding_agent_entry`
16. `a_managed_session_refuses_a_harness_that_cannot_withdraw_its_native_tools` — `FAILED_PRECONDITION` naming `codex-acp`
17. `opening_a_coding_agent_conversation_launches_it_as_the_session_user_on_the_session_worktree` — `session_os_user == "testdev"`, `worktree == repo_path`
18. `a_managed_session_launches_the_coding_agent_with_only_tddy_tools_exec_tools` — `managed`, MCP servers `["tddy-tools"]`, recorded on `session/new`
19. `a_coding_agent_turn_answers_through_prompt_agent_conversation` — `"fixed it"`, `end_turn`
20. `ending_a_coding_agent_conversation_stops_its_process`

Cypress (`mountWithRpc` / per-daemon in-memory backends):

21. `CreateSessionAgentPicker.cy.tsx` — `a coding agent row shows its coding-agent badge and harness` (fails: no badge); `a model row shows no coding-agent badge` (negative, passes)
22. `SessionAgentRosterStatus.cy.tsx` — `a roster row for a coding agent shows its badge` (fails: no badge)

**Changed from the plan:**
- *A coding agent owned by another daemon is refused at attach* is **moved to n8** `remote-codebase`: a remote attach resolves through the peer's `ListSubagents` before the row's kind is known, so on a one-daemon harness it cannot be distinguished from "no peer".
- *A long coding-agent turn is awaitable* (grace → `responseId` → `subagent_await`) is an MCP-layer behaviour of `tddy-tools` that the daemon RPC does not have; it is covered generically by `tddy-tools-cli/tests/subagent_async_response_acceptance.rs` and gains nothing coding-agent specific. Replaced by test 19.
- *A record written before kinds loads as a model entry* is guaranteed by the serde default in the published surface (`#[serde(default)] kind: AgentKindRecord`); a test would pass at contract, so none was added.
- `SubagentError` is a string newtype, not an enum: the refusal is the constructor `SubagentError::unsupported_by_coding_agent(option)`, not a variant.
- The launcher port is `CodingAgentLauncher::command_for(&CodingAgentLaunch) -> AcpAgentCommand`; `CodingAgentSession::start(command, open, model)` spawns it, so the session is testable without the daemon. The daemon seam is `DaemonSessionHost::with_coding_agent_launcher`, carried into `AgentRoster.coding_agent_launcher`.

**Baseline (2026-10-24, `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`):** `cargo check --all-targets` clean for `tddy-discovery tddy-coding-subagent tddy-changeset tddy-session-agents tddy-daemon-rpc tddy-session-lifecycle tddy-tools tddy-daemon tddy-session-split`; `cargo clippy --all-targets -- -D warnings` clean for the touched crates; the existing specs in the two Cypress files pass.

## Technical Debt & Production Readiness

**Green (2026-10-24)** — session, catalog, admission, open/turn/end, the web badge and the daemon
launcher are real and covered by the acceptance tests and the launcher's unit tests. One launcher
gap remains, as a typed refusal naming its reason (never a weaker launch):

- **Another OS user on a supervised host — done, with a gap in what could be proven.** A new
  `tddy-supervisor` request, `SpawnRelayed` (see Decisions), starts an allowlisted harness as the
  session's user with stdin/stdout relayed over a unix socket the daemon listens on. The daemon side
  (`coding_agent_relay`, used by `DaemonCodingAgentLauncher::launch_transport`) binds a 0700-directory
  socket, asks the supervisor, accepts its connection and returns the stream, which
  `AcpDriver::spawn_over` serves (`CodingAgentSession::start_over`). A privileged unsupervised daemon
  still uses `setpriv` (`coding_agent_as_user`); the daemon's own user needs neither.
  **Not provable here without root / on this host:** that the child really runs as a *different* uid
  (the tests run the target user as the current user, so no privilege is dropped — the drop is the
  pre-exec plan's own, separately tested contract); and the supervisor spawning + relaying end to end
  on this macOS host (`spawn_broker` refuses `PR_SET_PDEATHSIG` off Linux, so the live-child test
  `an_allowlisted_harness_has_its_stdio_relayed_and_its_exit_reported` is `#[cfg(target_os = "linux")]`
  and was **written but not run** — CI runs it). Proven here: the policy and peer gates refuse before
  any fork (4 acceptance tests against the real supervisor binary), the socket-ownership and
  peer-credential checks, byte relay both ways and caller-hangup (unit tests over a real `cat`), the
  driver over a stream against the stub, and the daemon asking the supervisor with the right fields
  (a fake supervisor that connects like the real one), plus the typed refusal.
  Operator setup: `spawn_policy.allowed_relay_tool_paths` must list each harness binary (absolute),
  `allowed_session_users` the user, and `allowed_env_keys` every variable a launch sets (a route's
  variables, `OPENCODE_CONFIG_CONTENT`) — a missing one is the supervisor's opaque denial, which the
  daemon turns into an error naming those three settings.
- **Provider account** — done. `route_from_vault` resolves the profile's account in the session
  owner's vault with `resolve_session_route` (the call tool sessions use) and the launcher applies
  it per harness: env + secret env for all; `-c` overrides (`codex_acp_route_args`, now exported from
  `tddy-agent-backend` — an additive export) for `codex-acp`; the route's config file as inline
  `OPENCODE_CONFIG_CONTENT` for `opencode`. An unresolvable route refuses; it never falls back to the
  vendor. Verified by unit tests with a fake resolver; the vault read itself is the tool sessions'
  tested path.
- **Managed launches** — done. `claude-acp`: `OpenSession.meta` =
  `{claudeCode:{options:{disallowedTools:[Read,Write,Edit,MultiEdit,NotebookEdit,Bash,BashOutput,KillShell,Glob,Grep,LS]}}}`
  (verified: reaches `session/new` on the stub, per #720's test; **documented, not verified** against
  a live `claude-agent-acp`). `opencode`: `OPENCODE_CONFIG_CONTENT` = `{"tools":{bash,read,write,edit,patch,glob,grep,list:false}}`
  merged over the route's config (**documented, not verified** against a live OpenCode).
  `tddy-tools --mcp` is handed over with `TDDY_REMOTE_*` from `colocated_jail_tool_env` and a token
  minted for the agent (`tddy-session-split::coding_agent_wiring`); a daemon that signs no tokens
  refuses. `CodingAgentLauncher::mcp_servers_for` (default: the launch's own) is how the launcher
  completes the servers.
- `subagent_cancel` while a turn runs ends the conversation (the table's `closed` signal drops the
  turn and the session, which kills the process). `CodingAgentSession::cancel_handle` is real
  (`session/cancel`, turn ends `cancelled`) but the conversation table holds `Box<dyn SubagentSession>`
  and cannot reach it; wiring it is n3's table work.
- A provider-account-less coding agent's model is `default_model_for_agent(id)` — a model id the
  harness may not advertise; `session/set_model` then fails loudly naming the offered models.
- `tddy-daemon-rpc --test in_jail_conversation_acceptance` fails on this macOS host (the jailed
  `tddy-sandbox-runner` exits 126 before its ready marker); its four sibling sandbox targets pass.
  This branch touches no jail, runner, supervisor or spawn file — environment, not a regression.

### Confinement mechanism per harness (what `confinement_for` relies on)

See *Managed launches* above; `codex-acp` offers sandbox modes only, no tool withdrawal:
`CannotWithdraw`, refused in managed sessions at attach and again at launch.

## Decisions & Trade-offs

- **Score 5 on purpose.** The boundary was considered: splitting the web badge out would leave a node
  whose agents are invisible in the picker, and splitting the launcher out would leave a session with
  no process. Kept as one node; the badge is small.
- **Confinement per harness is partly unsettled.** Withdrawing native tools is harness-specific:
  `claude-agent-acp` takes `disallowedTools` through `session/new` `_meta`; `opencode` through its
  config file's tool permissions; `codex-acp` has no tool withdrawal, only sandbox modes. The table
  (`HarnessConfinement`) starts with what the red phase can prove against each real adapter's
  documentation; anything unproven is `cannot_withdraw`, which refuses it in managed sessions (never a
  silent unconfined launch). Revisit at 6e.
- **Deny, not allow, until n3.** An auto-approving subagent on the main agent's worktree is the
  behaviour this stack exists to avoid.
- **Running as the session's OS user is the launcher's job, and its mechanism is open.** The driver
  needs piped stdio to the child; whether the daemon's spawn worker / supervisor can hand back piped
  stdio for a non-PTY child is unverified. If it cannot, green records it and stops rather than
  running the agent as the daemon's own user.
- **A supervised host gets the user switch from a new, separately allowlisted supervisor request.**
  See *Privileged-surface change* — the alternative (extending `SpawnSession` with stdio) would have
  widened the grant of every session tool.
- **Process per conversation.** One ACP child per conversation keeps cancel and kill trivially scoped;
  a pooled agent with several ACP sessions is a later optimisation.

## Privileged-surface change (recorded explicitly)

n2 adds one request to the root-owned `tddy-supervisor` surface: **`SpawnRelayed`**
(`packages/tddy-supervisor`: `proto/supervisor.proto`, `src/relay.rs`, `policy::resolve_relay_tool_path`,
`SpawnPolicy.allowed_relay_tool_paths`). What it can and cannot do:

- It spawns an allowlisted binary as an allowed session user, never root, with the session-spawn
  environment gate. **Its own allowlist, `allowed_relay_tool_paths`, is empty by default** and is not
  `allowed_tool_paths`: a host that allowlisted session tools has not enabled it, and no other caller
  gains a capability.
- It is reachable only by a peer that already passes the authorizer (a declared service's uid).
- The one new action of a root process is connecting to a socket path the caller names. That is a
  confused-deputy edge, closed twice: before connecting, the path must be absolute and plain, `lstat`
  a socket (a symlink is refused, not followed) owned by the caller's uid; after connecting, the
  listener's `SO_PEERCRED` uid must equal the caller's. Either failure is the opaque `Denied`, and the
  socket is connected **before** the fork, so a refusal forks nothing.
- The relay moves bytes between that socket and the child's piped stdio; stderr is logged at debug,
  not relayed. A caller that closes the connection stops the session (`StopSession` semantics);
  status and exit come from the existing `SessionStatus`.
- `ForkBroker` gained `spawn_piped` beside `spawn`; existing spawns are unchanged.

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

## TODO

- [x] Record initial discovery (`2026-10-24-coagents-acp-subagent-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived by the developer 2026-10-24: "finish the remaining nodes without stopping")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (launcher gaps open, see Technical Debt)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-coding-subagent -p tddy-session-agents -p tddy-daemon-rpc -p tddy-changeset`; the two Cypress specs) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p <each> -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
