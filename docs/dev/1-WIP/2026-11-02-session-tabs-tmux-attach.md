# Changeset: Attach a running tmux session as a session tab

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — proto + `tddy-terminal-rpc` + `tddy-cli-sessions` + daemon + coder participant + web; a first run-as-the-session-user tab; tests need a real tmux
**Stack:** `#session-tabs 3/7` — branch `feature/session-tabs/tmux-attach`, base `feature/session-tabs/agent-history`
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

- [ ] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): proto (`ListTmuxSessions`, `StartTerminalSessionRequest.tmux_session`, `TerminalSessionInfo.tmux_session`), `TerminalKind::Tmux`, the `TmuxHost` port
- [ ] [`tddy-cli-sessions`](../../../packages/tddy-cli-sessions/README.md): `start_tmux_terminal` spawns as a given OS user and records the tmux session name
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): the daemon's `HostTmux` (list as user, attach argv) and the roster arm
- [ ] [`tddy-coder`](../../../packages/tddy-coder/README.md): refuses kind `tmux` and `ListTmuxSessions`
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): the tmux section of `NewTerminalMenu`, `tmux: <name>` labels

## Responsibility

- **Proto** (`packages/tddy-terminal-rpc/proto/terminal_session.proto`):
  - `rpc ListTmuxSessions(ListTmuxSessionsRequest{session_token, session_id}) returns (ListTmuxSessionsResponse{repeated TmuxSessionInfo sessions})`;
  - `TmuxSessionInfo{name, windows: uint32, attached: uint32}`;
  - `StartTerminalSessionRequest.tmux_session: string`;
  - `TerminalSessionInfo.tmux_session: string`.
- `TerminalKind::Tmux { session: String }`, from kind `"tmux"` with a non-empty `tmux_session`. A
  `tmux` kind without a name → `invalid_argument`.
- `TerminalDescriptor.tmux_session: Option<String>`.
- `tddy_terminal_rpc::service::TmuxHost` trait:
  `async fn list(&self, os_user: &str) -> Result<Vec<TmuxSession>, Status>`.
  - Added to `TerminalSessionPorts` as `tmux`.
  - The service answers `ListTmuxSessions` through it.
- `CliSessionManager::start_tmux_terminal(session_id, worktree, os_user, argv, tmux_session)`, which goes
  through `spawn_tool` with `os_user: Some(_)`.
- Daemon `HostTmux` (`tddy-session-lifecycle/src/connection_service/svc_tmux_host.rs`):
  - lists by running `tmux list-sessions -F '#{session_name}\t#{session_windows}\t#{session_attached}'`
    as the user;
  - "no server running" → `[]`, tmux missing → `failed_precondition`;
  - `tmux_attach_argv(binary, name) = [binary, "-T", "clipboard", "attach-session", "-t", "=<name>"]`
    (`=` forces an exact match);
  - an unknown name is checked against `list` first → `not_found`;
  - the tmux binary comes from config (`tmux_binary`, default `tmux` on PATH).
- `CliManagerTerminalRoster::start` `Tmux` arm. The worktree comes from the live `main`, or from n1's
  resolution when stopped.
- The coder participant: `ListTmuxSessions` and kind `tmux` → `failed_precondition`.
- **Web:**
  - `useTmuxSessions` (`src/components/sessions/useTmuxSessions.ts`) fetches on menu open;
  - the `NewTerminalMenu` tmux section (`sessions-terminal-tab-new-tmux-<name>`, the empty state
    `sessions-terminal-tab-new-tmux-empty`, an inline error);
  - `useSessionTerminals.open` accepts `{kind: "tmux", tmuxSession}`;
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
| `n1` dormant-tabs | `TerminalLaunch.os_user`; roster start on a stopped session (`recorded_worktree`) | the tmux arm spawns as `launch.os_user`; a tmux tab on a stopped session gets its worktree the same way | add or change `TerminalLaunch.os_user`, or the stopped-session lookup |
| `n2` agent-history | `recorded_history` port on `TerminalSessionPorts` | constructs `TerminalSessionPorts` with it in place | touch recording or `RecordedTerminalHistory` |

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
- `packages/tddy-session-lifecycle/src/connection_service/svc_tmux_host.rs`
- `packages/tddy-terminal-rpc/src/service/tmux.rs` (the port and the `ListTmuxSessions` handler)
- `packages/tddy-web/src/components/sessions/useTmuxSessions.ts`

### Modified
- `terminal_session.proto` (+ generated TS in `tddy-web` / `tddy-rpc-web` as the repo's codegen does).
- `tddy-terminal-rpc` `service/ports.rs`, `service.rs`.
- `tddy-cli-sessions` `terminals.rs`.
- `svc_terminal_ports.rs`.
- the `tddy-coder` participant.
- `tddy-daemon` config (`tmux_binary`).
- `tddy-web` `NewTerminalMenu.tsx`, `useSessionTerminals.ts`, `SessionTerminalTabs.tsx` (label), `cypress/support/rpc/terminalSessionServiceBackend.ts`.
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/web-terminal.md`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/tmux_tabs_acceptance.rs` (each test runs a private tmux server on its own `-L` socket via the configured binary)

- [ ] `list_tmux_sessions_returns_the_users_sessions_with_window_and_attached_counts`
- [ ] `list_tmux_sessions_without_a_tmux_server_is_empty`
- [ ] `list_tmux_sessions_without_tmux_installed_is_refused_naming_tmux`
- [ ] `a_tmux_tab_attaches_to_the_named_session_and_carries_input_and_output`
- [ ] `stopping_a_tmux_tab_detaches_and_leaves_the_tmux_session_running`
- [ ] `a_tmux_tab_for_an_unknown_session_is_not_found`
- [ ] `list_terminal_sessions_reports_a_tmux_tab_with_its_tmux_session_name`

**`tddy-web`**: `packages/tddy-web/cypress/component/SessionTerminalTabsTmuxAcceptance.cy.tsx`

- [ ] `"+" lists the host's tmux sessions`
- [ ] `choosing a tmux session opens a tab labelled "tmux: <name>"`
- [ ] `"+" shows "No tmux sessions" when there are none`
- [ ] `"+" shows the error when tmux is unavailable`

### Unit / integration tests

- [ ] `tddy-terminal-rpc` `TerminalKind::from_request`: `tmux` + name → `Tmux`; `tmux` without a name → refused; other kinds are unchanged.
- [ ] `svc_tmux_host.rs`:
  - the list-format parser (tabs, empty output, the "no server running" stderr);
  - `tmux_attach_argv` uses `-T clipboard` and an exact `=name` target.
- [ ] `packages/tddy-coder/tests/` (extended parity or a new case): the coder refuses `ListTmuxSessions` with `failed_precondition`.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Listing, attach, detach | Real tmux on a private socket | A fake tmux would not prove detach-on-close or `-T clipboard` acceptance |
| Menu | Cypress component, in-memory backend | UI only |

## Decisions & Trade-offs

- **Run as the session's OS user**, the user's own tmux server, as chosen in the interview.
- **Per-client `-T clipboard`** rather than setting `set-clipboard` or `terminal-features` on the user's
  server, so tddy never edits the user's tmux.
- **⚠ Dependency question for pass 2:** the daemon tests need `tmux` in the dev shell
  (`flake.nix`). That is a new toolchain dependency — ask before adding it.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-tmux-attach-initial-discovery.md`)
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
