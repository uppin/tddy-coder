# Changeset: Attach a running tmux session as a session tab

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — proto + `tddy-terminal-rpc` + `tddy-cli-sessions` + daemon + coder participant + web; a first run-as-the-session-user tab; tests need a real tmux
**Stack:** `#session-tabs 3/7` — branch `feature/session-tabs/tmux-attach`, base `feature/session-tabs/agent-history` · **PR:** [#806](https://github.com/uppin/tddy-coder/pull/806)
**PRD:** [PRD-2026-11-02-tmux-attach-tab.md](../../ft/web/1-WIP/PRD-2026-11-02-tmux-attach-tab.md)

## Initial Discovery

[2026-11-02-session-tabs-tmux-attach-initial-discovery.md](./2026-11-02-session-tabs-tmux-attach-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | ⚠ During — a tmux tab must run as the session's OS user. It uses **the same spawn path the main agent PTY uses** (`spawn_tool(…, os_user: Some(_))`), so it works wherever the main agent does. Where that path refuses (`CannotBecomeUser`), the tab is refused with that error, never started as the daemon user |
| [2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md](../todo/2026-09-11-the-coder-terminal-coordinate-serves-seven-of-nine.md) | ⚠ During — the coder participant refuses `ListTmuxSessions` (`failed_precondition`: tool sessions run bash only). The record's table gains that row at wrap |
| [2026-10-24-reattach-does-not-replay-the-alternate-screen.md](../todo/2026-10-24-reattach-does-not-replay-the-alternate-screen.md) | ℹ Answered for tmux tabs — tmux runs on the alternate screen, and the web's `SNAPSHOT` open carries it, so a reattaching browser renders the tmux screen correctly. Non-web replays are untouched |

## Affected Packages

- [x] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): proto (`ListTmuxSessions`, `StartTerminalSessionRequest.tmux_session`, `TerminalSessionInfo.tmux_session`), `TerminalKind::Tmux`, the `TmuxHost` port
- [x] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): `start_tmux_terminal` spawns as a given OS user and records the tmux session name
- [x] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the daemon's `HostTmux` (list as user, attach argv) and the roster arm
- [x] [`tddy-coder`](../../../packages/tddy-coder/README.md): refuses kind `tmux` and `ListTmuxSessions`
- [x] [`tddy-web`](../../../packages/tddy-web/README.md): the tmux section of `NewTerminalMenu`, `tmux: <name>` labels

## Responsibility

- **Proto** (`packages/tddy-terminal-rpc/proto/terminal_session.proto`):
  - `rpc ListTmuxSessions(ListTmuxSessionsRequest{session_token, session_id}) returns (ListTmuxSessionsResponse{repeated TmuxSessionInfo sessions})`;
  - `TmuxSessionInfo{name, windows: uint32, attached: uint32}`;
  - `StartTerminalSessionRequest.tmux_session: string`;
  - `TerminalSessionInfo.tmux_session: string`.
- `TerminalKind::Tmux { session: String }`, and `TerminalKind::from_request(kind, tmux_session) ->
  Result<TerminalKind, Status>`: kind `"tmux"` (`TMUX_KIND`) with a non-empty `tmux_session` → `Tmux`;
  a `tmux` kind without a name → `invalid_argument`.
- `TerminalLaunch.os_user: String` — the OS user the request's token maps to, filled by
  `start_terminal_session` (planned in n1, moved here in pass 2 as its only consumer).
- `TerminalDescriptor.tmux_session: Option<String>`.
- `tddy_terminal_rpc::service::{TmuxHost, TmuxSession, NoTmux}` — the trait
  `async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status>`, and `NoTmux`, a host
  that refuses every listing with `failed_precondition`.
  - Added to `TerminalSessionPorts` as `tmux`.
  - The service answers `ListTmuxSessions` through it (`list_tmux_sessions`, already wired in the
    contract: it is a pass-through).
- `CliSessionManager::start_tmux_terminal(session_id, worktree, os_user, argv, tmux_session)`, which goes
  through `spawn_tool` with `os_user: Some(_)`.
- Daemon `HostTmux` (`connection_service/svc_terminal_ports/tmux_host.rs`, a submodule so
  `connection_service.rs` gains no `mod` line):
  - lists by running `tmux list-sessions -F LIST_SESSIONS_FORMAT` (name, windows, attached; parsed
    by `parse_list_sessions`) as the user;
  - "no server running" → `[]`, tmux missing → `failed_precondition`;
  - `tmux_attach_argv(binary, name) = [binary, "-T", "clipboard", "attach-session", "-t", "=<name>"]`
    (`=` forces an exact match);
  - an unknown name is checked against `list` first → `not_found`;
  - the binary is `TDDY_TMUX_BINARY` when set and non-empty, else `tmux` on PATH (`tmux_binary()`),
    the same pattern as `TDDY_CLAUDE_BINARY`. A `DaemonConfig` field was rejected: the struct has
    ~150 construction sites.
- `CliManagerTerminalRoster::start` `Tmux` arm. The worktree comes from the live `main`, or from n1's
  resolution when stopped.
- The coder participant: `ListTmuxSessions` (`NoTmux`) and kind `tmux` → `failed_precondition`. Both
  refusals land in the contract commit, so a tool session can never start a shell for a `tmux` kind.
- **Web:**
  - `useTmuxSessions({sessionId, sessionToken, client}) -> TmuxMenuState {sessions, error, refresh}`
    (`src/components/sessions/useTmuxSessions.ts`) lists on menu open;
  - `NewTerminalMenu` / `SessionTerminalTabs` take `tmux?: TmuxMenuState` and
    `onOpen(kind, tmuxSession?)`;
  - the `NewTerminalMenu` tmux section (`sessions-terminal-tab-new-tmux-<name>`, the empty state
    `sessions-terminal-tab-new-tmux-empty`, an inline error);
  - `useSessionTerminals.open(kind, tmuxSession?)`; `SessionTerminal.tmuxSession`;
  - the tab label is `tmux: <name>`.

## Boundaries

- No `tmux new-session` (n7).
- No change to bash or harness tab identity (still the daemon user).
- No tmux server option is modified.
- No OSC 52 handling in the web (n4); this node only asks tmux for it (`-T clipboard`).
- No killing tmux sessions.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` dormant-tabs | roster start on a stopped session (`recorded_worktree`) | a tmux tab on a stopped session gets its worktree the same way | change the stopped-session lookup |
| `n2` agent-history | `recorded_history` port on `TerminalSessionPorts`, `NoRecordedHistory` | constructs `TerminalSessionPorts` with it in place (and adds `tmux` beside it) | touch recording or `RecordedTerminalHistory` |

## Draft PR contract

The first push publishes:

- the proto additions (regenerated);
- `TerminalKind::Tmux`, `TerminalDescriptor.tmux_session`, the `TmuxHost` trait and port field;
- `start_tmux_terminal` and `HostTmux` as `// TODO(tmux-attach): implement`;
- the web hook and menu props;
- the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — its daemon tests run a private tmux server (`tmux -L <socket>`) as the test user; the web tests use the in-memory backend
**Concurrent with:** n1 `dormant-tabs`, n2 `agent-history`, n4 `osc52-clipboard`, n5 `tab-plus`
**Blocks:** n7 `tmux-new` — it adds a second argv on this node's tmux spawn path, kind plumbing and menu section

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A

- tmux appears nowhere in the codebase. Started tabs run as the daemon user (`os_user: None`), with
  kinds `bash` or a harness. `TerminalKind::from_request` maps any non-bash kind to `Agent`.
- `TerminalSessionInfo` carries `terminal_id`, `kind` and `pid` only.
- The "+" menu lists Bash plus harnesses.

## State B

- "+" lists the user's tmux sessions. Choosing one opens `tmux: <name>`, attached as the session's
  OS user with the clipboard feature.
- Closing the tab detaches. Every viewer sees the same label from `ListTerminalSessions`.

## Delta

### New
- `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports/tmux_host.rs`
- `packages/tddy-web/src/components/sessions/useTmuxSessions.ts`
- The port lives in `tddy-terminal-rpc/src/service/ports.rs` beside the others; the handler in
  `service.rs`.

### Modified
- `terminal_session.proto` (+ generated TS in `tddy-web` / `tddy-rpc-web` as the repo's codegen does).
- `tddy-terminal-rpc` `service/ports.rs`, `service.rs`.
- `tddy-cli-sessions` `terminals.rs`.
- `svc_terminal_ports.rs`.
- the `tddy-coder` participant.
- `flake.nix`: `pkgs.tmux` in the dev shell (approved by the developer 2026-11-02), so the tests and
  CI have a real tmux.
- Every `StartTerminalSessionRequest { … }` and `TerminalDescriptor { … }` literal gains the new field.
- `tddy-web` `NewTerminalMenu.tsx`, `useSessionTerminals.ts`, `SessionTerminalTabs.tsx` (label), `cypress/support/rpc/terminalSessionServiceBackend.ts`.
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/web-terminal.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/tmux_tabs_acceptance.rs`. Each test
runs a private tmux server: `TMUX_TMPDIR` points tmux's socket directory at a temp dir, for the daemon
and the test alike. The identity maps to the OS user running the test, so no privilege change happens.
Serial, because the tests set process env.

- [x] `list_tmux_sessions_returns_the_users_sessions_with_window_and_attached_counts`
- [x] `list_tmux_sessions_without_a_tmux_server_is_empty`
- [x] `list_tmux_sessions_without_tmux_installed_is_refused_naming_tmux`
- [x] `a_tmux_tab_attaches_to_the_named_session_and_carries_input_and_output`
- [x] `stopping_a_tmux_tab_detaches_and_leaves_the_tmux_session_running`
- [x] `a_tmux_tab_for_an_unknown_session_is_not_found`
- [x] `list_terminal_sessions_reports_a_tmux_tab_with_its_tmux_session_name`

**`tddy-web`**: `packages/tddy-web/cypress/component/SessionTerminalTabsTmuxAcceptance.cy.tsx`

- [x] `"+" lists the host's tmux sessions`
- [x] `choosing a tmux session opens a tab labelled "tmux: <name>"`
- [x] `"+" shows "No tmux sessions" when there are none`
- [x] `"+" shows the error when tmux is unavailable`

### Unit / integration tests

- [x] `packages/tddy-terminal-rpc/tests/terminal_kind_unit.rs`:
  - `a_tmux_kind_with_a_session_is_a_tmux_terminal_attached_to_it`;
  - `a_tmux_kind_without_a_session_is_refused_as_invalid_argument`;
  - `bash_and_harness_kinds_are_read_as_before` (a guard).
- [x] `tmux_host.rs` unit tests: `the_listing_reads_name_windows_and_attached_clients_per_line`,
  `a_session_name_with_spaces_is_kept_whole`, `an_empty_listing_has_no_sessions`,
  `a_tab_attaches_with_the_clipboard_feature_to_exactly_the_named_session`.
- The coder's refusals are already in place (contract), and `two_server_parity_acceptance` stays green
  with `NoTmux` on both servers.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Listing, attach, detach | Real tmux on a private socket | A fake tmux would not prove detach-on-close or `-T clipboard` acceptance |
| Menu | Cypress component, in-memory backend | UI only |

## Decisions & Trade-offs

- **Run as the session's OS user**, the user's own tmux server, as chosen in the interview.
- **Per-client `-T clipboard`** rather than setting `set-clipboard` or `terminal-features` on the user's
  server, so tddy never edits the user's tmux.
- **Listing goes through the kernel's `run_output_as_user`** (absolute tmux path from `tmux_binary()`; a bare name is looked up on the daemon's `PATH`, because the kernel anchors relative programs to the daemon's cwd). Exit 0 parses; "no server running" / "error connecting" is `[]`; a missing binary is `failed_precondition` naming tmux; a `CannotBecomeUser` is refused with that error. The child inherits the daemon's environment, so the daemon's own `TMUX_TMPDIR` decides which socket directory tmux reads. The roster's `Tmux` arm uses this list for its `not_found` check. Follow-up: `docs/dev/todo/2026-11-02-tmux-listing-and-tab-use-two-privilege-drops.md` (the listing's `pre_exec` drop and the tab's `PtyRuntime`/`setpriv` drop can disagree).
- **Tab refusal before spawn:** the roster arm calls `ensure_reachable` (kernel `resolve_target_account` + `plan_privilege_change_from`) and refuses with the named `CannotBecomeUser` message (`failed_precondition`); a tmux tab is never spawned as the daemon user. Unit-tested with an injected unprivileged identity, so no root is needed.
- **Accepted deviation:** `TmuxMenuState.loading` stays in the web hook, so the menu never flashes a false "No tmux sessions" while the listing is in flight.
- **`pkgs.tmux` in the dev shell** — asked and approved 2026-11-02, rather than a fake tmux script that
  could not prove detach-on-close or `-T clipboard`.

## Baseline before green

Recorded at the contract commit (2026-11-02), scoped to `tddy-terminal-rpc`, `tddy-cli-sessions`, `tddy-session-lifecycle`, `tddy-coder` (+ `tddy-web` specs):

- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are clean.
- **This node's red tests:**
  - `terminal_kind_unit.rs`: 2 failing (+ the bash/harness guard passing);
  - the four `tmux_host` unit tests failing (`an_empty_listing_has_no_sessions` passes on the empty stub);
  - `tmux_tabs_acceptance.rs`: 7 failing;
  - `SessionTerminalTabsTmuxAcceptance.cy.tsx`: 4 failing ("no `sessions-terminal-tab-new-tmux-*`").
- **Unchanged and green:** `terminal_session_acceptance` (12), `agent_terminal_tabs_acceptance` (7), `tddy-coder` `two_server_parity_acceptance` and the `connection_service_participant` lib tests, `SessionTerminalTabsAgentKindsAcceptance.cy.tsx` (7).
- **Inherited reds** (n1/n2's, expected on this branch): `recorded_worktree` units, `capture_recorder`, `recorded_history_acceptance`.
- **Not this node:** `tddy-terminal-rpc` `local_pty_relay::runs_a_fast_exiting_command_to_completion` hung for minutes in one package-wide run while another worktree loaded the machine. It passed alone in 0.01 s.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-tmux-attach-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — waived by the developer 2026-11-02 (reviewed with the stack at the end of pass 2)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [x] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [x] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
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
