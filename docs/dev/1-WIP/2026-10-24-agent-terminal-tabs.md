# Changeset: Agent terminal tabs — any CLI harness beside bash, in the session's worktree

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — a proto contract change (`StartTerminalSessionRequest.kind`) plus a port-trait
signature change (`TerminalRoster::start`) with three implementations, across five packages
(terminal-rpc, session-lifecycle, cli-sessions, coder, web); real-PTY integration tests
**Stack**: `#warp-agent` 2/2 — branch `feature/warp-agent/agent-terminal-tabs`, base `feature/warp-agent/warp-cli-session` (#717)
**PR**: [#718](https://github.com/uppin/tddy-coder/pull/718)

## Initial Discovery

[2026-10-24-agent-terminal-tabs-initial-discovery.md](./2026-10-24-agent-terminal-tabs-initial-discovery.md).
Exploration 3 is this node's map of the bash-tab mechanism end to end.

## Prerequisites

Step 2b found **no** `**Claimed by:**` record in any package this node touches, and no
`docs/dev/todo/` entry in its path. Code-issue records near it, none in a file this node edits:

- `packages/tddy-terminal-rpc/docs/code-issues/complexity-bridge-open-replay-ack-live.md`: — unrelated (`bridge.rs` replay; this node edits `service.rs` and the proto)
- `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md`: — unrelated (the roster is in `connection_service/svc_terminal_ports.rs`, not `connection_service.rs`)
- `packages/tddy-web/docs/code-issues/oversized-file-ghostty-terminal*.md`: — unrelated (the tab strip, not Ghostty)

Packages **not analyzed** (no `docs/code-issues/`): `tddy-cli-sessions`.

A pre-existing gap stays untouched on purpose. `StartTerminalSession` documents a `control_token`
but does not check it (`TODO(#unbundle-node-6)`, `tddy-terminal-rpc/src/service.rs:369`). An agent
kind inherits the same behaviour; it is not fixed here.

## Responsibility

This PR owns **choosing what a terminal tab runs**:

- Proto: `StartTerminalSessionRequest.kind = 4`. Empty or `"bash"` is the login shell; otherwise a
  CLI-harness session type. The proto comments on `TerminalSessionInfo.kind` and
  `StartTerminalSessionResponse` are widened to match.
- `tddy-terminal-rpc`: `TerminalLaunch { kind: TerminalKind, login_shell: String }`, with
  `TerminalKind::{Bash, Agent(String)}` parsed from the request. `TerminalRoster::start(&self,
  session_id, launch: &TerminalLaunch)` replaces `start(session_id, shell_path)`. The in-memory test
  roster records launches.
- `tddy-session-lifecycle`: `CliManagerTerminalRoster` resolves `TerminalKind::Agent` through
  `cli_harness(kind)` and `DaemonConfig::cli_harness_binary`, and builds the descriptor's fresh
  interactive argv. A Claude-style session id is minted per tab. An unknown kind is `INVALID_ARGUMENT`.
- `tddy-cli-sessions`: `CliSessionManager::start_agent_terminal(session_id, worktree, kind, argv)`,
  which runs `spawn_tool` under a fresh UUIDv7 terminal id with the kind as given.
- `tddy-coder`: `CoderTerminalRoster` refuses `TerminalKind::Agent` with `INVALID_ARGUMENT` ("this
  session's terminals run bash only").
- `tddy-web`:
  - `useSessionTerminals` holds `{ terminalId, kind }[]` and `open(kind)`.
  - `SessionTerminalTabs` gets the `+` menu and kind-aware labels with per-kind ordinals.
  - The agent options come from one exported list shared with the Create pane's session-type toggle.
  - The fake terminal backend records the requested kind and echoes it on list.

## Boundaries

- **Does not create, change or register a harness.** `cli_harness.rs`, every descriptor
  (including `WARP_CLI`) and `CliLaunch` are read only here. Warp is node 1's
  (`feature/warp-agent/warp-cli-session`, #717).
- **No persistence or resume** of any tab. `SessionMetadata` is untouched, including node 1's
  `harness_resume_token`. An agent tab's exit is never recorded.
- **No session environment** for an agent tab (no git identity, no `TDDY_SOCKET`, no provider route,
  no `os_user`), matching bash tabs. No change to how `DeleteSession` or `SignalSession` treat tabs.
- **No sandbox tab**: `DaemonTerminalSessionStore`'s jail rule (main only) is unchanged.
- **No hooks or activity status** for an agent tab. `.claude/settings.local.json` is not written or
  changed per tab.
- The Agent (`main`) tab, roster agents and conversation tabs are untouched.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**.
Implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` warp-cli-session (#717) | `WARP_CLI` registered fifth in `CLI_HARNESSES`. `{ id: "warp-cli", label: "Warp" }` in the web session-type toggle list | `cli_harness("warp-cli")` resolves like any harness. The web menu lists Warp because it reads the same list | add, change or test Warp's argv, its resume token, or `SessionMetadata`; assert Warp-specific launch behaviour |

**Sequencing fact.** Node 1's wave-2 contract publishes `WARP_CLI` with real descriptor values but a
`todo!()` `warp_argv`. This node's tests therefore launch **Muse Code and OpenCode** (existing,
implemented descriptors) as their agent kinds, never Warp. The one Warp-related assertion is
surface-only: the menu *lists* "Warp". That keeps every test here greenable while node 1 is still
red.

## Draft PR contract

The second commit (wave 2) publishes:

- `string kind = 4;` on `StartTerminalSessionRequest`, regenerated for Rust (`build.rs`) and the web
  (`src/gen/terminal_session_pb.ts`). It is real: generated code.
- `TerminalLaunch`, `TerminalKind`, and the new `TerminalRoster::start` signature with every
  implementation migrated. The bash path keeps working (real), and the agent path is
  `todo!("TODO(agent-terminal-tabs)")`, so the workspace builds and today's bash tests stay green.
- `CliSessionManager::start_agent_terminal` with a `todo!()` body.
- Web: the regenerated `terminal_session_pb.ts` (with `kind`), the menu test ids
  (`sessionsTerminalTabNewMenu`, `sessionsTerminalTabNewOption(kind)`), the page-object helpers
  (`newTabMenu`, `newTabOption`), and the fake backend recording `startedTerminalKinds` and listing
  the requested kind. `{ terminalId, kind }` state, `open(kind)`, the menu and the shared option list
  are implementation, pinned by the failing Cypress spec.
- Every acceptance test below, failing for this node's missing implementation.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes. Tests launch Muse/OpenCode stubs and mount the tab strip against an
in-memory backend. None of them exercises node 1's behaviour.
**Concurrent with:** #717 (`#warp-agent` 1/2).
**Blocks:** nothing.

Real dependency edges: none (the line `n1 → n2` is flattening only; this node consumes `n1`'s
*published* descriptor list, not its behaviour).

## Affected Packages

- **tddy-terminal-rpc**: [README.md](../../../packages/tddy-terminal-rpc/README.md) · [docs/terminal-session-service.md](../../../packages/tddy-terminal-rpc/docs/terminal-session-service.md): the `kind` field and the `TerminalRoster::start` launch request
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md): the daemon roster resolves agent kinds through the harness registry
- **tddy-cli-sessions**: [README.md](../../../packages/tddy-cli-sessions/README.md): `start_agent_terminal`
- **tddy-coder**: [README.md](../../../packages/tddy-coder/README.md): its roster refuses agent kinds
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) · [docs/terminal-session.md](../../../packages/tddy-web/docs/terminal-session.md): the `+` menu, kind-aware tabs

## Related Feature Documentation

- [PRD-2026-10-24-agent-terminal-tabs.md](../../ft/web/1-WIP/PRD-2026-10-24-agent-terminal-tabs.md)
- [Session terminal tabs](../../ft/web/session-terminal-tabs.md), [Terminal sessions](../../ft/daemon/terminal-sessions.md)

## Summary

Lets a terminal tab run any registered CLI harness instead of the login shell. `StartTerminalSession`
gains a `kind`. The daemon builds that harness's fresh interactive argv from its descriptor and spawns
it in the session's worktree as an extra tab. The web `+` becomes a menu, and tabs are labelled by
kind.

## Background

See the PRD. Technically, bash is the only kind at every layer today. The request has no kind field;
the RPC layer resolves the login shell and hands `TerminalRoster::start` a `shell_path`;
`CliSessionManager::start_terminal` hardcodes kind `"bash"`. The CLI harness registry (#667) already
knows how to build any harness's argv and resolve its binary, so the missing piece is a field and a
dispatch, not a launcher.

## Scope

- [ ] **Proto**: `kind = 4`, regenerated for Rust and the web
- [ ] **Port**: `TerminalLaunch` / `TerminalKind`, and the `TerminalRoster::start` signature across three implementations
- [ ] **Daemon**: agent kind → descriptor → binary → argv → `start_agent_terminal`; unknown kind refused
- [ ] **Coder**: refuses agent kinds
- [ ] **Web**: `+` menu; kind-aware labels with ordinals, restored on reattach; Bash only on a tool session; refusal shown
- [ ] **Docs**: `session-terminal-tabs.md`, `terminal-sessions.md`, and one line in each harness doc, at wrap; a TODO for the claude-cli-tab hooks limitation

## Technical Changes

### State A (Current)

- `StartTerminalSessionRequest { session_token = 1, session_id = 2, control_token = 3 }`.
- `TerminalRoster::start(&self, session_id: &str, shell_path: &str)`. The handler computes
  `login_shell_for(&os_user)`.
- Daemon (`svc_terminal_ports.rs:142-158`): main terminal → `worktree_path` →
  `manager.start_terminal(session_id, worktree, shell_path)` → `spawn_tool(…, "bash", …, argv=[shell],
  env=[], None)`.
- Coder (`terminal_session_service.rs:236-288`): `TerminalManager::start_terminal`, kind `"bash"`.
- `ListTerminalSessions` reports the kind from the PTY registry (`resolve_pty_handle`), so it already
  works for any kind `spawn_tool` was given.
- Web: `terminals: string[]`, `open()` with no arguments, label `terminalId.replace(/-/g, " ")`, and a
  single `+` button (`sessions-terminal-tab-new`).

### State B (Target)

- `StartTerminalSessionRequest.kind = 4`. `TerminalKind::from_request(&str) -> TerminalKind` maps
  `""`/`"bash"` to `Bash`, and anything else to `Agent(kind)`. Whether the agent kind is registered is
  decided by the roster, which knows the registry.
- `TerminalRoster::start(&self, session_id, launch: &TerminalLaunch)`.
- Daemon: `Bash` behaves as today. `Agent(kind)` goes through `cli_harness(&kind)`:
  - `None` is `INVALID_ARGUMENT` "unknown terminal kind {kind:?}".
  - Otherwise `binary = config.cli_harness_binary(h.session_type, h.default_binary, h.binary_env)`,
    `argv = (h.argv)(&CliLaunch { binary, session_id: <fresh uuid>, ..CliLaunch::default() })`, then
    `manager.start_agent_terminal(session_id, worktree, h.session_type, argv)`.
- Coder: `Agent(_)` is `INVALID_ARGUMENT`.
- Web: `terminals: { terminalId, kind }[]` and `open(kind: string)`.
  - `+` toggles `sessions-terminal-tab-new-menu`, with items `sessions-terminal-tab-new-bash` and
    `sessions-terminal-tab-new-<sessionType>` for each CLI harness option. A tool session lists Bash
    only.
  - Labels are `"<Kind label> <n>"`, numbered by kind in open order.
  - Escape or a click outside closes the menu.

### Delta (What's Changing)

#### tddy-terminal-rpc
- `proto/terminal_session.proto`: the field and comments.
- `src/service.rs`: the `TerminalLaunch` and `TerminalKind` types; the trait signature; the handler
  builds the launch.
- `tests/support/mod.rs`: `InMemoryRoster` records `TerminalLaunch`es (it replaces `started_shells()`
  with `started_launches()`, and existing callers migrate).

#### tddy-session-lifecycle
- `src/connection_service/svc_terminal_ports.rs`: `CliManagerTerminalRoster` holds the
  `DaemonConfig` and dispatches on the kind.

#### tddy-cli-sessions
- `src/cli_session_manager/terminals.rs`: `start_agent_terminal`.

#### tddy-coder
- `src/session_participant/terminal_session_service.rs`: the refusal.

#### tddy-web
- `src/gen/terminal_session_pb.ts` (regenerated); `useSessionTerminals.ts`; `SessionTerminalTabs.tsx`.
- `CreateSessionTypeToggle.tsx`: `SESSION_TYPE_OPTIONS` moves to an exported module (for example
  `src/components/sessions/cliHarnessOptions.ts`) that both components read. One list, so the menu
  cannot drift from the Create pane.
- `SessionRuntime.tsx`: passes the session type to the tab strip.
- `cypress/support/rpc/terminalSessionServiceBackend.ts`; `cypress/support/pages/sessionTerminalTabsPage.ts`;
  `cypress/support/testIds.ts`.

## Implementation Milestones

- [ ] Proto plus port plus the bash path migrated; existing terminal suites green unchanged
- [ ] Daemon agent dispatch: `agent_terminal_tabs_acceptance.rs` green
- [ ] Coder refusal plus the two-server parity test green
- [ ] Web menu and labels: `SessionTerminalTabsAgentKindsAcceptance.cy.tsx` green; existing tab specs green

## Testing Plan

### Testing Strategy

**Primary level: integration (daemon, real PTY, stub harness binaries).** What matters is that the
daemon picks the right binary (through the same resolution its own sessions use), builds the
descriptor's argv, and runs it in the session's worktree under a fresh id that `ListTerminalSessions`
reports with its kind. The harness is `terminal_session_acceptance.rs`'s
(`DaemonSessionHost::terminal_session_service()`, stub main), with `cli_harnesses.<type>.binary_path`
pointing at a `a_stub_agent_script` stub that records its argv and cwd. Muse and OpenCode are the
kinds under test (see Dependencies).

**RPC level** (`tddy-terminal-rpc`, in-memory roster): the request's kind reaches the roster, and an
empty kind arrives as `Bash` with the login shell.

**Coder unit**: an agent kind is refused, and bash still starts.

**Web component** (`mountWithRpc` + `anInMemoryRpcBackend`, via `aSessionServiceBackend`): menu, kind
on the wire, labels, reattach labels, tool session, refusal, Escape.

### Reliability

- Wait on the stub's recorded argv file and on `ListTerminalSessions` state with the suites'
  `eventually` / `a_capture_showing`. Never sleep.
- `#[serial_test::serial]` on PTY suites, as their siblings do.

### Coverage

- Happy path: an agent tab runs, is listed, and is stopped.
- Edge cases: an empty kind and `"bash"` both give the shell; two agent tabs of one kind get distinct
  ids; a claude-style harness gets its own session id, not the session's.
- Errors: unknown kind; agent kind on the coder; web refusal surfaced.

## Acceptance Tests

### tddy-terminal-rpc — `packages/tddy-terminal-rpc/tests/terminal_session_service_acceptance.rs`

Red state, verified 2026-10-24: the agent-kind test fails; the two bash-path tests pass (the bash path is kept).

- [ ] **Integration**: `start_terminal_session_hands_the_roster_the_requested_agent_kind`
- [ ] **Integration**: `a_start_without_a_kind_hands_the_roster_the_login_shell`
- [ ] **Integration**: `a_start_naming_bash_hands_the_roster_the_login_shell`

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/agent_terminal_tabs_acceptance.rs`

Red state, verified 2026-10-24: 7 of 7 fail. The RPC handler still hands the roster `Bash`, so a shell starts instead of the harness and an unknown kind is not refused. The existing `terminal_session_acceptance.rs` passes 12/12.

- [ ] **Integration**: `an_agent_terminal_runs_that_harness_in_the_sessions_worktree` — Muse stub; argv `["muse"]`, cwd = the main terminal's worktree
- [ ] **Integration**: `an_agent_terminal_is_listed_with_its_harness_as_its_kind`
- [ ] **Integration**: `an_agent_terminal_gets_a_fresh_id_distinct_from_main_and_from_other_tabs`
- [ ] **Integration**: `an_agent_terminal_resolves_its_binary_from_the_harness_config` — `cli_harnesses.opencode-cli.binary_path` stub is the one spawned
- [ ] **Integration**: `a_claude_agent_terminal_starts_a_conversation_of_its_own` — `--session-id` is present and is not the session's id
- [ ] **Integration**: `an_unknown_terminal_kind_is_refused_and_nothing_is_spawned`
- [ ] **Integration**: `stopping_an_agent_terminal_kills_it_and_removes_it_from_the_list`

### tddy-coder — `packages/tddy-coder/src/session_participant/connection_service_participant/tests.rs`
- [ ] **Unit**: `start_terminal_session_refuses_an_agent_kind`

### tddy-web — `packages/tddy-web/cypress/component/SessionTerminalTabsAgentKindsAcceptance.cy.tsx`

Red state, verified 2026-10-24: 7 of 7 fail (no `sessions-terminal-tab-new-menu`). The existing `SessionTerminalTabsAcceptance.cy.tsx` passes 5/5.

- [ ] **Component**: "opens a menu of Bash and every CLI harness from '+'"
- [ ] **Component**: "starts a Muse Code terminal from the menu and focuses its tab"
- [ ] **Component**: "labels tabs by kind with a per-kind ordinal"
- [ ] **Component**: "restores tab labels from the listed terminals' kinds on reattach"
- [ ] **Component**: "offers only Bash on a tool session"
- [ ] **Component**: "closes the menu on Escape without starting a terminal"
- [ ] **Component**: "shows why an agent terminal could not start and opens no tab"
- [ ] **Component (migrated)**: `SessionTerminalTabsAcceptance.cy.tsx` and `SessionTabsVisibleErrorsAcceptance.cy.tsx` open bash through the menu (`sessions-terminal-tab-new` → `sessions-terminal-tab-new-bash`) and keep their assertions

## Technical Debt & Production Readiness

- [ ] **Existing tab specs need the menu**: `SessionTerminalTabsAcceptance.cy.tsx`, `SessionTabsVisibleErrorsAcceptance.cy.tsx` and `SessionTerminalFullscreenAcceptance.cy.tsx` click a bare `+` (`tabs.newTab()`) and expect a bash tab. Once `+` opens a menu, they must go through `newTabOption("bash")`. Migrate them in green; they are left unmigrated in the contract so they keep proving the bash path until then.
- [ ] The daemon roster needs the `DaemonConfig` to resolve a harness binary: `CliManagerTerminalRoster::new` gains it (wired in `terminal_session_ports`).

## Decisions & Trade-offs

- **Any registered harness, not Warp only** (developer, 2026-10-24). The registry already builds every
  argv, so the extra kinds cost tests, not code.
- **A kind, not a command line.** The request names a registered session type, never an argv, so a
  client cannot make the daemon run an arbitrary binary through a tab. Binary resolution stays the
  operator's (`daemon.yaml` / env).
- **Fresh interactive start only.** No model, prompt, resume or provider route. A tab is a scratch
  agent, and its argv is exactly what the descriptor builds for `CliLaunch::default()` plus a binary
  and a session id.
- **One shared option list** for the Create toggle and the tab menu, so a sixth harness appears in
  both.
- **Labels change for bash tabs too** (from the id to "Bash N"). Daemon tab ids are UUIDv7, so the
  current label is unreadable in production.
- **Known limitation, recorded as a TODO at wrap**: a claude-cli tab in a claude-cli session shares
  the worktree's hooks file, so its activity may be attributed to the main session.

## Refactoring Needed

### From @red (TDD Red Phase)

### From @validate-changes (Change Validation)

### From @validate-tests (Test Quality)

### From @prod-ready (Production Readiness)

### From @analyze-clean-code (Code Quality)

## Validation Results

_(populated by validation commands)_

## Predecessor PRs

- `#warp-agent` 1/2: `feature/warp-agent/warp-cli-session` (#717)

## TODO

- [x] Record initial discovery (`2026-10-24-agent-terminal-tabs-initial-discovery.md`)
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
