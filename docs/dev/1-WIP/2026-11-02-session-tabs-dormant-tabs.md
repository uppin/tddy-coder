# Changeset: A stopped session keeps its tabs — shells, harness terminals and agents

**Created:** 2026-11-02
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — two packages (`tddy-session-lifecycle`, `tddy-web`); a new runtime-less tab host in the web, and the dormant base view changes for every stopped PTY session
**Stack:** `#session-tabs 1/7` — branch `feature/session-tabs/dormant-tabs`, base `master` · **PR:** [#804](https://github.com/uppin/tddy-coder/pull/804)
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

- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `CliManagerTerminalRoster::start` resolves a stopped session's worktree from its metadata
- [ ] [`tddy-web`](../../../packages/tddy-web/README.md): a dormant session renders a tab strip; the transcript moves into its Agent tab

## Responsibility

- `CliManagerTerminalRoster::start` (`svc_terminal_ports.rs`): when `manager.get(session_id)` is `None`,
  start the tab in the worktree `recorded_worktree` resolves. The roster gains the daemon's
  `tddy_data_dir`.
- `recorded_worktree(tddy_data_dir, session_id) -> Result<PathBuf, Status>`, a submodule of
  `svc_terminal_ports` (`connection_service/svc_terminal_ports/recorded_worktree.rs`, so
  `connection_service.rs` gains no `mod` line):
  - validates the id as one path segment (`invalid_argument`);
  - reads `tddy_core::repo_root_for_session(<tddy_data_dir>/sessions/<id>)`, i.e. the changeset's
    worktree, then `.session.yaml` `repo_path`;
  - `None` → `failed_precondition` naming the session. There is no substitute directory.
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

- `recorded_worktree` as `// TODO(dormant-tabs): implement`, answering `unimplemented`, and already
  called by the roster when `main` is absent (so a stopped session is still refused until green);
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
- `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports/recorded_worktree.rs`
- `packages/tddy-web/src/components/sessions/DormantSessionRuntime.tsx`

### Modified
- `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` (`start` resolves via `recorded_worktree` when `main` is absent; the roster gains the config it needs).
- `packages/tddy-web/src/components/sessions/SessionMainPane.tsx` (render `DormantSessionRuntime` for a dormant session).
- Docs, at wrap: `docs/ft/web/session-terminal-tabs.md`, `docs/ft/web/inactive-session-activities.md`, `packages/tddy-session-lifecycle/docs/`.

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

- [ ] `recorded_worktree.rs` unit tests (`#[cfg(test)]`):
  - `a_stopped_session_resolves_to_the_worktree_its_metadata_records`;
  - `a_session_whose_metadata_records_no_worktree_is_refused_naming_it`;
  - `a_session_with_no_directory_on_this_host_is_refused_naming_it`;
  - `a_session_id_that_is_not_one_path_segment_is_refused`.

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
- **The session dir is the daemon's, not the user's** (`<tddy_data_dir>/sessions/<id>`, as the agent
  roster resolves it), so the roster needs no OS user. `TerminalLaunch.os_user` was planned here and
  moved to n3 `tmux-attach`, its only consumer (refined in pass 2).
- **A render slot for the Agent tab**, so n6 replaces its body without touching the strip host.

## Baseline before green

Recorded at the contract commit (2026-11-02), scoped to `tddy-session-lifecycle` + `tddy-terminal-rpc`:

- `cargo build -p … --all-targets`, `cargo fmt --check`, `cargo clippy -p … --all-targets -- -D warnings`: clean.
- **This node's red tests:** `dormant_session_tabs_acceptance.rs` 4 failing (+ the live-session regression guard passing); the four `recorded_worktree` unit tests failing; `DormantSessionTabsAcceptance.cy.tsx` 5 failing ("no `sessions-terminal-tabs`").
- **Not this node — environmental under a bare `cargo test -p`** (sibling binaries not built; `./test` builds `tddy-tools`): `ensure_project_home_session_acceptance`, `ensure_project_home_session_replace_edges`, `project_home_idle_stop_acceptance` (`tddy-tools not found in target/debug`); every `sandboxed_*` suite, `sandbox_behavior_acceptance`, `claude_cli_fullscreen_renderer` (3), `cursor_cli_session_acceptance` (1) (`env: tddy-sandbox-runner: No such file`); `session_sync_livekit_acceptance` (`tddy-remote-git-repo is not built`); `tool_session_host_socket` (7); lib `stack_child_spawn_tests::a_child_started_from_the_dialog_is_told_to_read_its_changeset`.

## TODO

- [x] Record initial discovery (`2026-11-02-session-tabs-dormant-tabs-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests — approved by the developer 2026-11-02 (later nodes' reviews waived)
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
