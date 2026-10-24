# 2026-10-24 — main is set up with the New-session form
**Type:** Fix

`EnsureProjectHomeSession` takes an optional start spec built by the New-session form and records
the started session's id on the project (`ProjectEntry.home_session_id`); without a spec it reaches
the recorded session. The `home_agent` setting, `SetProjectHomeAgent` and the Projects-card picker
are removed. The setup pane is the New-session form in a main variant, and the pinned entry gains
**Replace main**.

The bug: the picker offered every `ListAgents` row and `SetProjectHomeAgent` stored any listed id,
but ensure accepted only a `claude-cli` **harness profile**, so the catalog's own agents could be
stored and never started (`home agent "claude" is not a harness profile`).

Decisions (developer unless noted):
- main stays read-only: only `claude-cli` may start it; the web shows the other types disabled with
  the reason, and the daemon refuses them itself, checking the type **after** the harness profile
  named by `agent` is applied so a profile cannot swap one in.
- The project references main by session id. Ensure carries the spec; the role stays
  daemon-granted, and the daemon overwrites the location fields (project, home worktree, branch,
  stack, managed mode, role) instead of refusing them. A placement (`sandbox`,
  `sandboxed_codebase`, `codebase_daemon_instance_id`, `ssh_config_host`, `agent_clone`,
  `split_agent`) and a requested role are refused.
- A stale recorded id is cleared and answered `NOT_FOUND`, which the web reads as "open setup".
- Replace main stops a served recorded main after every start check that can fail without side
  effects has passed; the old session keeps its record and role.
- The ensure lock is taken before `home_session_id` is read, so two set-ups record one main
  (`started` + `replaced`).
- No adoption of a main that existed before the project recorded one (planner, approved with the
  PRD); users set main up once again.

Permanent docs:
- [session-drawer.md § Pinned main entry, § Main variant](../../ft/web/session-drawer.md#pinned-main-entry)
- [project-concept.md § Project-home session](../../ft/daemon/project-concept.md#project-home-session)
- [projects-screen-multi-host.md § Main session](../../ft/web/projects-screen-multi-host.md#main-session)
- [url-state-routing.md](../../ft/web/url-state-routing.md), [harness-profiles.md](../../ft/daemon/harness-profiles.md)
- package docs: [tddy-agent-launch module-layout](../../../packages/tddy-agent-launch/docs/module-layout.md),
  [tddy-projects project-service](../../../packages/tddy-projects/docs/project-service.md),
  [tddy-daemon-rpc architecture](../../../packages/tddy-daemon-rpc/docs/architecture.md),
  [tddy-web projects-screen](../../../packages/tddy-web/docs/projects-screen.md)

Package entries: tddy-service, tddy-agent-launch, tddy-projects, tddy-daemon-rpc, tddy-daemon-kernel,
tddy-session-lifecycle, tddy-web (`docs/changesets/2026-10-24-project-home-from-new-session-form.md` in each). Mechanical
`home_agent` → `home_session_id` literal renames in tddy-daemon, tddy-daemon-livekit,
tddy-session-files, tddy-session-activity, tddy-telegram-control, tddy-worktree-service.

Code issues, re-measured at `4348f160a` against the merge-base `c1df8e142` with `/analyze-code-issues` (commit `f3f1c93ac`): no finding is attributable to this change. `start_session_on_request` (`complexity-svc-start-session-core-start-session-core`)
improved from 393 to 384 lines: two inline empty-model checks became one call to
`refuse_a_cli_start_without_a_model`. Every other record in tddy-agent-launch, tddy-daemon-rpc and
tddy-projects is unchanged by this branch; the four first detections
(`crap-project-coordinate-handlers-create-project-at-project-coordinate` CRAP 240,
`crap-project-coordinate-handlers-list-project-branches-at-project-coordinate` CRAP 110,
`crap-project-provision-resolve-remote-git-repo-path` CRAP 182,
`crap-project-provision-ensure-project-available-from-facilitator` CRAP 90) are pre-existing,
never-entered functions this branch touches at most by a field rename. No record closed; none deleted.

Backlog: none resolved. Kept, touched not fixed: `2026-10-23-session-drawer-grew-past-500-by-project-groups`,
`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode` (note added: the main form shows
Cursor CLI disabled for this reason), `2026-10-23-project-home-running-heuristic` (note added: a
main that outlived a daemon restart cannot be stopped by Replace main), and the code issue
`oversized-file-project-coordinate-handlers` (field rename only, 586 production lines at both).
Deferred to the backlog: `2026-10-24-sandboxed-main-needs-session-role`,
`2026-10-24-project-card-setup-link-ignores-card-host`,
`2026-10-24-harness-profile-empty-account-not-vendor` (found outside this scope).

Verification was local and scoped to the touched packages: tddy-session-lifecycle
`ensure_project_home_session_acceptance` 22 and `project_home_idle_stop_acceptance` 5,
tddy-projects `project_home_session_storage` 5, tddy-daemon-rpc `project_home_session_entry` 2 and
`rpc_handlers_shape` 10, tddy-service `unbundle_service_split` 28, `cargo test -p tddy-agent-launch
--lib` 152; clippy `-D warnings` and fmt clean on tddy-agent-launch, tddy-daemon-rpc, tddy-projects,
tddy-session-lifecycle; tddy-web `bun test` 693; Cypress component, 8 specs, 108 passed.
Whole-workspace health is CI's.
