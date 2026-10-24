# Changeset: main is set up with the New-session form

**Date**: 2026-10-24
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — proto contract change (Ensure `start`, `ProjectEntry.home_session_id`, RPC removed), five crates + tddy-web, stored-data migration (`home_agent` dropped)

## Initial Discovery

Full codebase exploration that grounded this plan: [initial-discovery.md](./2026-10-24-project-home-from-new-session-form-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

Code issues: none claimed in `tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-projects`, `tddy-web`,
`tddy-service`, `tddy-session-lifecycle`. `tddy-projects` has no `docs/code-issues/` (not analyzed).

### ⚠ DURING — `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-project-coordinate-handlers.md`

`coordinate_handlers.rs` holds two `ProjectData` literals; this change only renames a field there.
Must not grow the file.

### ⚠ DURING — [2026-10-23-session-drawer-grew-past-500-by-project-groups.md](../todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md)

`SessionDrawer.tsx` is over budget; `SessionsDrawerScreen.tsx` (490) and `CreateSessionPane.tsx`
(483) are at it. The main variant, the Replace action and the stale-main redirect live in new
modules (`projectHomeSessionTypes.ts`, `ProjectHomeSetupPane.tsx`, `useEnsureProjectHome.ts`); the
three big files grow only by wiring. Recorded, not fixed here.

### ⚠ DURING — [2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md)

Why only `claude-cli` may start main. The disabled-type reason for Cursor CLI cites it. Not fixed here.

### ⚠ DURING — [2026-10-23-project-home-running-heuristic.md](../todo/2026-10-23-project-home-running-heuristic.md)

"Running" stays pid-or-terminal, now for the **recorded** session. Its "what would close it" test
path (`ensure_project_home_session_acceptance.rs`) is rewritten here; the heuristic is not changed.

### — Unrelated

[2026-10-23-project-home-git-push-remote-and-token.md](../todo/2026-10-23-project-home-git-push-remote-and-token.md),
[2026-10-23-project-home-auto-stop-untracked-processes.md](../todo/2026-10-23-project-home-auto-stop-untracked-processes.md).

## Affected Packages

- **tddy-service**: [README.md](../../packages/tddy-service/README.md) — `session.proto` (Ensure `start = 4`, outcome `replaced`), `project.proto` (`SetProjectHomeAgent` removed, `home_agent = 10` reserved, `home_session_id = 11`)
- **tddy-agent-launch**: [README.md](../../packages/tddy-agent-launch/README.md) — `svc_ensure_project_home.rs` starts from the spec, records/reads the id
  - [module-layout.md](../../packages/tddy-agent-launch/docs/module-layout.md)
- **tddy-projects**: [README.md](../../packages/tddy-projects/README.md) — `ProjectData.home_session_id`, `set_project_home_session`; `home_agent` + handler/service method removed
  - [project-service.md](../../packages/tddy-projects/docs/project-service.md)
- **tddy-daemon-rpc**: [README.md](../../packages/tddy-daemon-rpc/README.md) — `home_agent_handler.rs` removed, `entries.rs` maps `home_session_id`
  - [architecture.md](../../packages/tddy-daemon-rpc/docs/architecture.md)
- **tddy-daemon-kernel**: `peer_calls::forward_set_project_home_agent` removed
- **tddy-session-lifecycle**: [session-service.md](../../packages/tddy-session-lifecycle/docs/session-service.md) — Ensure contract
- **tddy-web**: [README.md](../../packages/tddy-web/README.md) — main variant of the New-session form, setup pane, pinned entry + Replace main, Projects card
  - [projects-screen.md](../../packages/tddy-web/docs/projects-screen.md)
- Mechanical `ProjectData` / `ProjectEntry` literal rename in: tddy-daemon, tddy-daemon-livekit, tddy-session-files, tddy-session-activity, tddy-telegram-control, tddy-worktree-service (tests/src listed in discovery)

## Related Feature Documentation

- [PRD-2026-10-24-project-home-from-new-session-form.md](../../ft/web/1-WIP/PRD-2026-10-24-project-home-from-new-session-form.md)
- [session-drawer.md § Pinned main entry](../../ft/web/session-drawer.md#pinned-main-entry), [project-concept.md § Project-home session](../../ft/daemon/project-concept.md#project-home-session), [projects-screen-multi-host.md § Home agent](../../ft/web/projects-screen-multi-host.md#home-agent)

## Summary

`EnsureProjectHomeSession` takes an optional start spec built by the New-session form and records
the started session's id on the project; without a spec it reaches the recorded session. The
`home_agent` setting, `SetProjectHomeAgent` and the Projects-card picker are removed. The setup pane
is the New-session form in a main variant, and the pinned entry gains **Replace main**.

## Background

The picker offered `ListAgents` rows; ensure accepted only `claude-cli` harness profiles, so the
catalog's own agents could be stored and never started (`home agent "claude" is not a harness
profile`). The developer asked for main to be configured like any session, with New-session
options, and for the project to reference the session id.

## Scope

- [ ] **Daemon**: Ensure with/without `start`; refusals; replace; stale-id clearing; id recorded
- [ ] **Storage**: `home_session_id` replaces `home_agent`
- [ ] **Removal**: `SetProjectHomeAgent` end to end (proto, handler chain, peer forward, `RESIDUAL_METHODS`, web)
- [ ] **Web**: main variant of the form; type-availability rule; setup pane; pinned entry + Replace main + stale redirect; Projects card link
- [ ] **Docs**: PRD wrapped into the three feature docs and the package docs
- [ ] **Testing**: acceptance + unit tests below passing

## Technical Changes

### State A

See discovery § Combined Conclusions. In short: ensure builds a fixed `claude-cli` request from a
harness profile named by `ProjectData.home_agent`; the home session is found by role; the web picks
`home_agent` from `ListAgents` on the Projects card and the setup pane.

### State B

**Proto**
- `EnsureProjectHomeSessionRequest { …; StartSessionRequest start = 4; }` — absent: reach the
  recorded main; present: set up or replace it. Outcome gains `"replaced"`.
- `ProjectEntry`: `reserved 10; reserved "home_agent"; string home_session_id = 11;` (empty = none).
- `ProjectService.SetProjectHomeAgent` and its messages removed.

**Daemon (`svc_ensure_project_home.rs`)**
- No `start`: read `project.home_session_id`. None → `NOT_FOUND` ("project X has no main session:
  set one up"). Recorded but no session dir → clear it (`set_project_home_session(.., None)`), same
  `NOT_FOUND`. Otherwise running/resumed as today, for that id.
- With `start` (`checked_home_spec`):
  - `session_type != "claude-cli"` → `FAILED_PRECONDITION` naming the type (`"tool"` for empty) and
    that main could not be kept read-only.
  - `sandbox || sandboxed_codebase || !codebase_daemon_instance_id.is_empty()` → `INVALID_ARGUMENT`.
  - `!session_role.is_empty()` → `PERMISSION_DENIED` (mirrors `refuse_a_requested_role`).
  - The daemon sets `session_token`, `project_id`, `repo_path` (home worktree), `managed_codebase =
    true`, `session_role = project-home`, and clears the branch fields
    (`branch_worktree_intent`, `selected_branch_to_work_on`, `new_branch_name`, `base_branch`,
    `create_remote_branch`) and `daemon_instance_id`; everything else from the spec is kept
    (model, `provider_account_id`, `provider_built_in`, `agent`, `permission_mode`,
    `dangerously_skip_permissions`, `initial_prompt`, `specialized_agents`, `semantic_index`, …).
  - A recorded main that is served is stopped (`claude_cli_manager.stop_session`); its record stays.
  - `start_session_with_role`, then `set_project_home_session(.., Some(new_id))`. Outcome
    `replaced` when a recorded main existed, else `started`.
- `find_home_session` (role scan) and `home_agent_profile` are removed.

**Storage (`tddy-projects`)**
- `ProjectData.home_session_id: Option<String>` (serde default, skip when none); `home_agent`
  removed (an older row's key is ignored on read and not written back).
- `set_project_home_session(projects_dir, project_id, Option<&str>)`; `set_project_home_agent`
  removed.

**Web**
- `projectHomeSessionTypes.ts`: `whyMainCannotRun(type: SessionType): string | null` — `null` for
  `claude-cli`, a reason for each other type.
- `CreateSessionPane` gains `projectHome?: { projectId; daemonInstanceId; replacing; onStart(spec) }`
  (`createSessionPaneProps.ts`). When set: project/host fixed, host+project selects, branch, stack
  parent, attachments and placement toggles hidden; type toggle disables types with a reason
  (`create-session-type-reason-<type>`), initial type `claude-cli`; submit reads **Start main
  session** and calls `onStart(buildProjectHomeStartSpec(form))` instead of `StreamStartSession`.
- `createSessionRequest.ts`: `buildProjectHomeStartSpec(form)` — the claude-cli request without
  branch, placement or project fields.
- `ProjectHomeSetupPane` renders the title/explanation (+ replacing warning
  `sessions-project-home-setup-replacing-<id>`) around the variant form; `onStart` →
  `EnsureProjectHomeSession{start}` → select + attach the session (`handleSessionCreated`).
- Routing: `#/sessions/project-home/<id>/replace` opens the pane in replace mode.
- `ProjectHomeInfo { projectId; mainBranchRef; homeSessionId: string | null }` — configured iff
  non-null; `projectHomeSlot` finds the home session by id; the entry's label is that session's
  `model`. Configured entry: click → ensure (no spec); `NOT_FOUND` → reload projects + open setup.
  Configured entry shows **Replace main** (`sessions-drawer-project-home-replace-<id>`).
- Projects card: `ProjectHomeAgent` removed; `project-main-session-link-<id>` reads **Set up main**
  (→ `#/sessions/project-home/<id>`) or **Open main** (→ `#/sessions/<homeSessionId>`).
  `?project=` focus removed.

### Delta

| Package | Change |
|---|---|
| tddy-service | proto edits; `RESIDUAL_METHODS` 20 → 19 |
| tddy-agent-launch | ensure rewritten around the spec + recorded id |
| tddy-projects | field + setter swap; handler/service/lib method removed |
| tddy-daemon-rpc | `home_agent_handler.rs` + port + test double removed; `entries.rs` maps the id |
| tddy-daemon-kernel | `forward_set_project_home_agent` removed |
| tddy-web | gen regenerated; components/hooks above; `ProjectHomeAgent.tsx` deleted |
| others | `home_agent: None` → `home_session_id: None`; `home_agent: String::new()` → `home_session_id: String::new()` |

## Implementation Milestones

- [ ] Proto + gen (`scripts/generated-code.sh write packages/tddy-web` and the rust-typescript-tests tree)
- [ ] `ProjectData.home_session_id` + `set_project_home_session`; literal sweep (`cargo check --all-targets` per crate)
- [ ] Ensure with/without spec, refusals, replace, stale clearing
- [ ] `SetProjectHomeAgent` removed end to end
- [ ] `whyMainCannotRun` + `buildProjectHomeStartSpec`
- [ ] Form variant + setup pane + routing replace mode
- [ ] Pinned entry by id, label, Replace main, stale redirect
- [ ] Projects card link; picker deleted

## Testing Plan

**Levels.** The daemon contract is pinned at the RPC with a real `DaemonSessionHost`, a real git
repo and a stub `claude` (as today). Storage is pinned at `project_storage`. The web flow is pinned
by Cypress component specs mounting `SessionsDrawerScreen` / `SessionDrawer` / `ProjectsAppPage`
over in-memory RPC backends; the pure rule and spec builder by bun unit tests.

**Not tested here:** a real Claude binary; multi-host forwarding of the spec (unchanged generic
peer routing).

## Acceptance Tests

### tddy-session-lifecycle — `packages/tddy-session-lifecycle/tests/ensure_project_home_session_acceptance.rs` (rewritten)

- `setting_up_main_starts_a_managed_claude_cli_session_on_the_home_worktree_at_the_main_ref`
- `setting_up_main_records_the_session_on_the_project`
- `setting_up_main_runs_the_agent_with_the_specs_model_permission_mode_and_initial_prompt`
- `ensuring_main_while_it_runs_returns_the_recorded_session`
- `ensuring_a_stopped_main_fast_forwards_and_resumes_the_same_conversation`
- `ensuring_a_project_with_no_main_is_not_found_and_starts_nothing`
- `ensuring_a_recorded_main_that_no_longer_exists_clears_it_and_is_not_found`
- `setting_up_main_as_cursor_cli_is_refused_naming_the_type_and_starts_nothing`
- `setting_up_main_as_a_tool_session_is_refused_and_starts_nothing`
- `setting_up_main_in_a_sandbox_is_refused_and_starts_nothing`
- `setting_up_main_with_a_requested_role_is_refused_and_starts_nothing`
- `replacing_a_running_main_stops_it_and_records_the_new_session`
- `a_replaced_main_is_still_listed_with_its_role`
- kept: `a_project_home_session_is_listed_with_its_role`, `an_ensured_project_home_agent_is_pre_approved_the_project_home_tool_set`, `a_start_session_naming_a_role_is_refused_and_starts_nothing`

### tddy-session-lifecycle — `tests/project_home_idle_stop_acceptance.rs` (fixture only)

Fixture sets main up with a claude-cli spec instead of a `home_agent:` row; tests unchanged.

### tddy-projects — `packages/tddy-projects/tests/project_home_session_storage.rs` (replaces `project_home_agent_storage.rs`)

- `an_older_row_with_a_home_agent_reads_as_having_no_home_session`
- `a_stored_home_agent_is_not_written_back`
- `recording_a_home_session_stores_its_id`
- `clearing_the_home_session_writes_no_key`
- `recording_on_an_unknown_project_is_an_error`

### tddy-daemon-rpc — `packages/tddy-daemon-rpc/tests/project_home_session_entry.rs` (new)

- `list_projects_reports_the_recorded_home_session_id`
- `list_projects_reports_no_home_session_for_a_project_without_main`
- `set_project_home_agent_acceptance.rs` deleted

### tddy-service — `packages/tddy-service/tests/unbundle_service_split.rs`

- `RESIDUAL_METHODS` without `SetProjectHomeAgent` (existing test, list edited)

### tddy-web — `cypress/component/ProjectHomeSetupPaneAcceptance.cy.tsx` (rewritten)

- clicking main on a project with no main opens the New-session form for that project
- the main form hides the host, project, branch, stack, attachments and placement controls
- only Claude CLI can be chosen for main; every other type says why
- Start main session ensures main with the form's model, permission mode and initial prompt, and selects it
- shows the daemon's reason in the form when setting up main is refused
- Cancel closes the pane back to the drawer root
- says so when the host does not list the project, and offers nothing to start

### tddy-web — `cypress/component/SessionsDrawerProjectHomeAcceptance.cy.tsx` (adapted + new)

- each project group shows a pinned main entry first, labelled with main's model
- clicking a configured main ensures it without a start spec and selects it
- main is not repeated in its group's rows
- a project with no main starts nothing and shows no hint
- the Other group has no pinned entry
- a project whose main is not listed yet still shows its pinned entry
- a project with no main and no session gets no group
- a session with the project-home role that is not the recorded main is an ordinary row
- a set-up main offers Replace main; a project with no main does not
- Replace main asks to set that project's main up again, and starts nothing (`SessionDrawer.onReplaceProjectHome`)

### tddy-web — `cypress/component/ProjectHomeReplaceAcceptance.cy.tsx` (new)

- Replace main opens the setup form saying the current main will be stopped
- starting from the replace form ensures main with a start spec and selects the new session
- a main the daemon no longer has opens the setup form

### tddy-web — `cypress/component/ProjectHomeSetupLinkAcceptance.cy.tsx` (adapted)

- a project with no main links to Set up main from its Projects card
- a project with a main links to Open main from its Projects card
- `ProjectsScreenHomeAgentAcceptance.cy.tsx` deleted (picker removed)

### tddy-web — unit (bun)

- `src/components/sessions/projectHomeSessionTypes.test.ts`: claude-cli can run main; tool, cursor-cli, opencode-cli, muse-cli each have their reason
- `src/components/sessions/projectHomeStartSpec.test.ts`: `buildProjectHomeStartSpec` carries model, provider, permission mode, initial prompt; carries no project, branch or placement fields
- `src/utils/projectHomeGroups.test.ts`: adapted to `homeSessionId`; new: keeps a project-home session that is not the recorded main as a row

### Test support (tddy-web)

- `cypress/support/pages/projectHomeSetupPage.ts` (new page object); `cypress/support/testIds.ts`: `sessionsDrawerProjectHomeReplace`, `sessionsProjectHomeSetupReplacing`, `createSessionTypeReason`, `projectMainSessionLink`
- Green removes the now-unused `projectsScreenPage.homeAgent*` helpers and `projectHomeAgentSelect`/`projectHomeAgentError` ids

### Draft contract added with the tests

- Proto: `EnsureProjectHomeSessionRequest.start = 4`, `ProjectEntry.home_session_id = 11` (gen regenerated); `home_agent` still present — green removes it
- `ProjectData.home_session_id` (+ `home_session_id: None` in every literal); `project_storage::set_project_home_session` is a `todo!()`
- `entries.rs` leaves `home_session_id` defaulted (TODO)
- web stubs that throw: `whyMainCannotRun` (`projectHomeSessionTypes.ts`), `buildProjectHomeStartSpec` (`createSessionRequest.ts`)

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **Read-only kept** (developer): only `claude-cli`; other types shown disabled with a reason.
- **Project references the session id** (developer), replacing `home_agent` (developer: remove it).
- **Ensure carries the spec** (developer) — the role stays daemon-granted.
- **Stale id → clear + `NOT_FOUND`** (developer); `NOT_FOUND` is distinct from refusals so the web can open setup.
- **Replace main** (developer): stops the recorded main; the old session keeps its role and record.
- **Sandbox refused for main** (planner, approved with PRD): the sandboxed claude-cli path drops `session_role`. TODO to file at green: carry the role on the sandboxed path, then allow it.
- **No adoption of a pre-existing role-tagged main** (planner, approved with PRD): users set main up once again.
- **Daemon overwrites location fields** of the spec (project, repo path, branch, managed, role) rather than refusing them — they are the daemon's by definition of main; placement and role are refused because silently dropping them would change what the operator asked for.

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

- [x] Record initial discovery (`2026-10-24-project-home-from-new-session-form-initial-discovery.md`)
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
