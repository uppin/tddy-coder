# Changeset: A stopped session keeps its tabs — shells, harness terminals and agents

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — three packages (`tddy-terminal-rpc`, `tddy-session-lifecycle`, `tddy-web`); a new runtime-less tab host in the web; the launch port gains the OS user that n3 also builds on
**Stack:** `#session-tabs 1/7` — branch `feature/session-tabs/dormant-tabs`, base `master`
**PRD:** [PRD-2026-11-02-dormant-session-tabs.md](../../ft/web/1-WIP/PRD-2026-11-02-dormant-session-tabs.md)

## Initial Discovery

[2026-11-02-session-tabs-dormant-tabs-initial-discovery.md](./2026-11-02-session-tabs-dormant-tabs-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-08-30-agent-conversation-tabs-four-follow-ups.md](../todo/2026-08-30-agent-conversation-tabs-four-follow-ups.md) | ✅ **Partly RESOLVED HERE**: the bullet "The header's Add-agent button renders where there is no tab strip" is closed for **dormant** sessions (the strip now exists there). Workflow and PR-Stack sessions still have no strip, so at wrap that bullet is **narrowed** to them and the record is kept, not deleted |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ During — the change lives in `svc_terminal_ports.rs`; no `mod` / `pub use` line is added to `connection_service.rs` |
| `packages/tddy-web/docs/code-issues/oversized-file-sessions-drawer-screen.md` | ⚠ During — the dormant tab host is a new component; `SessionsDrawerScreen.tsx` gains no lines beyond wiring props through |
| `tddy-cli-sessions` | ℹ No `docs/code-issues/` directory: not analyzed. This node adds nothing there |

## Affected Packages

- [ ] [`tddy-terminal-rpc`](../../../packages/tddy-terminal-rpc/README.md): `TerminalLaunch` carries the requesting OS user
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `CliManagerTerminalRoster::start` resolves a stopped session's worktree from its metadata
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): a dormant session renders a tab strip; the transcript moves into its Agent tab

## Responsibility

- `tddy_terminal_rpc::service::TerminalLaunch.os_user: String` — the OS user the request's token maps to,
  filled by `start_terminal_session`.
- `CliManagerTerminalRoster::start` (`svc_terminal_ports.rs`): when `manager.get(session_id)` is `None`,
  read `<sessions_base_for_user(launch.os_user)>/<session_id>/.session.yaml` and start the tab in its
  `repo_path`. A missing session dir, metadata or `repo_path` →
  `failed_precondition("session <id> has no worktree on this host")`.
- A private helper `recorded_worktree(os_user, session_id) -> Result<PathBuf, Status>` in a new file
  `connection_service/svc_recorded_worktree.rs`.
- `tddy-web` `DormantSessionRuntime` (`src/components/sessions/DormantSessionRuntime.tsx`): the tab strip
  for a session with no live connection.
  - It reuses `SessionTerminalTabs`, `useSessionTerminals` and `GrpcSessionTerminal` against the
    owning daemon's terminal client, plus the agent-conversation tabs.
  - Its Agent tab body is a **render slot** `agentTab: React.ReactNode`, which `SessionMainPane` fills
    with `SessionActivitiesPane`. n6 fills the same slot with the greyed terminal.
- `SessionMainPane`: a dormant selected session renders `DormantSessionRuntime` instead of the
  full-pane activities overlay. A workflow session's custom view is unchanged.

## Boundaries

- No greyed terminal history and no toast (n6 `dormant-agent-tab`).
- No change to `ResumeSession` or its button.
- Tabs are not jailed and not persisted. A split session's agent half (no `repo_path`) is refused,
  never served from the session dir.
- The coder-participant roster (`tddy-coder`) is unchanged; it serves only its own live session.
- Does not change what kinds a tab can run (n3/n7 add tmux).

## Dependencies

None — this is the root node. It depends on `master` only.

## Draft PR contract

The first push publishes:

- `TerminalLaunch.os_user`, filled by `start_terminal_session`;
- `svc_recorded_worktree.rs` with `recorded_worktree` as `// TODO(dormant-tabs): implement` (the roster
  still refuses without `main`);
- `DormantSessionRuntime` with its props (including the `agentTab` slot), rendering nothing yet
  (`// TODO(dormant-tabs): implement`);
- the failing tests below.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — the daemon tests run this daemon's own roster against a session directory written by the test; the web tests mount against an in-memory RPC backend
**Concurrent with:** n2 `agent-history`, n3 `tmux-attach`, n4 `osc52-clipboard`, n5 `tab-plus`
**Blocks:** n6 `dormant-agent-tab` — its tests need a stopped session to render the tab strip and fill the `agentTab` slot this node adds

Real dependency edges, as opposed to the branch line:

    n1 → n6      n2 → n6      n3 → n7

## State A

- `CliManagerTerminalRoster::start` refuses every start without a live `main`
  (`failed_precondition("session has no running terminal")`); it needs `main` only for its
  `worktree_path`.
- `start_terminal_session` resolves `os_user` but hands the roster only `{ kind, login_shell }`.
- Web: `SessionMainPane` covers the runtime layer with `SessionActivitiesPane` for a dormant session,
  and passes `focused={!dormant && …}`, which hides `SessionRuntime` entirely. A dormant session with no
  runtime has no tab strip, no "+" and nowhere for an added agent's conversation tab to appear.
- "Add agent" is shown whenever a client exists; on a dormant session its conversation is invisible.

## State B

- Starting a tab on a stopped session runs it in the recorded worktree as today's tabs run: on the
  host, as the daemon user, with the session user's login shell.
- A stopped session in the web shows: **Agent** (transcript) · listed terminal tabs ·
  agent-conversation tabs · "+". Terminals stream through the daemon's `TerminalSessionService`.
- After Resume the live `SessionRuntime` takes over and lists the same daemon terminals.

## Delta

### New
- `packages/tddy-session-lifecycle/src/connection_service/svc_recorded_worktree.rs`
- `packages/tddy-web/src/components/sessions/DormantSessionRuntime.tsx`

### Modified
- `packages/tddy-terminal-rpc/src/service/ports.rs` (`TerminalLaunch.os_user`) and `src/service.rs` (fill it).
- `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` (`start` resolves via `recorded_worktree` when `main` is absent; the roster gains the config it needs).
- `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` (render `DormantSessionRuntime` for a dormant session).
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/inactive-session-activities.md`, `packages/tddy-terminal-rpc/docs/`.

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`**: `packages/tddy-session-lifecycle/tests/dormant_session_tabs_acceptance.rs`

- [ ] `a_bash_tab_on_a_stopped_session_starts_in_its_recorded_worktree`
- [ ] `a_harness_tab_on_a_stopped_session_starts_in_its_recorded_worktree`
- [ ] `a_stopped_session_without_a_recorded_worktree_is_refused_naming_it`
- [ ] `an_unknown_session_is_refused_naming_it`
- [ ] `a_live_session_still_starts_tabs_in_its_main_terminals_worktree`

**`tddy-web`**: `packages/tddy-web/cypress/component/DormantSessionTabsAcceptance.cy.tsx`

- [ ] `a stopped session shows its tab strip with the recorded transcript in the Agent tab`
- [ ] `"+" → Bash on a stopped session opens a working terminal tab`
- [ ] `a stopped session lists the terminals the daemon already runs for it`
- [ ] `Add agent on a stopped session opens a conversation tab in its strip`
- [ ] `a tab opened while the session was stopped is still in the strip after Resume`

### Unit / integration tests

- [ ] `packages/tddy-terminal-rpc/tests/terminal_session_service_acceptance.rs` (extended): `start_terminal_session_hands_the_roster_the_requesting_os_user`.
- [ ] `svc_recorded_worktree.rs` unit tests: `repo_path` is returned; missing dir, missing metadata and missing `repo_path` each refuse naming the session.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Worktree resolution and refusal | Daemon acceptance through `terminal_session_service()` with a real `CliSessionManager` and a temp sessions base | Exercises the served coordinate, not a lookalike roster |
| Dormant strip | Cypress component, `mountWithRpc` + `anInMemoryRpcBackend` | The strip's behaviour is the web's; the daemon is a double |

## Decisions & Trade-offs

- **Metadata is the source when `main` is gone**, not a cached path in the manager: the manager loses
  everything on daemon restart, the metadata does not.
- **No substitute directory.** Resume falls back to the session dir; tabs do not, because a shell in the
  wrong directory is worse than a clear refusal (CLAUDE.md: no fallbacks without consent).
- **A render slot for the Agent tab**, so n6 replaces its body without touching the strip host.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-dormant-tabs-initial-discovery.md`)
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
