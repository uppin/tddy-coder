# Changeset: multi-host-projects — dedicated Projects screen + add a project to another host

**Date:** 2026-07-03  
**Branch:** `multi-host-projects`  
**Packages:** `tddy-service`, `tddy-daemon`, `tddy-web`  
**Feature PRD:** [docs/ft/web/projects-screen-multi-host.md](../../ft/web/projects-screen-multi-host.md)

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Proto: add `AddProjectToHost` RPC + `AddProjectToHostRequest`/`AddProjectToHostResponse`; add `bool local_only` to `ListProjectsRequest` (`packages/tddy-service/proto/connection.proto`)
- [x] Regenerate Rust (`packages/tddy-service/build.rs`) + TS (`./dev bun run generate` → `packages/tddy-web/src/gen/connection_pb.ts`)
- [x] `project_storage::add_or_get_project` — idempotent append by `project_id` (`packages/tddy-daemon/src/project_storage.rs`)
- [x] ~~Generalize~~ `classify_peer_route` already existed and is generalized (`packages/tddy-daemon/src/livekit_peer_discovery.rs`) — reused as-is
- [x] `forward_add_project_to_host_via_livekit` next to `forward_start_session_via_livekit` (`livekit_peer_discovery.rs`)
- [x] `add_project_to_host` handler — auth, validate, route (local/forward), clone + persist reusing given `project_id` (`packages/tddy-daemon/src/connection_service.rs`)
- [x] `list_projects`: honor `local_only` (skip peer merge) (`connection_service.rs`)
- [x] Implement `LiveKitEligibleDaemonSource::peer_project_entries` — fan-out to peers' `ListProjects` with `local_only=true`, tag by `daemon_instance_id` (`livekit_peer_discovery.rs`); added `room_slot` to the source + `main.rs` wiring
- [x] Routing: `PROJECTS_ROUTE`/`isProjectsPath` (`tddy-web/src/routing/appRoutes.ts`); dispatch branch (`tddy-web/src/index.tsx`); nav item (`tddy-web/src/components/shell/DaemonNavMenu.tsx`)
- [x] `ProjectsAppPage` container — polls `listProjects` + `listEligibleDaemons`; wires `createProject` + `addProjectToHost` (`tddy-web/src/components/projects/ProjectsAppPage.tsx`)
- [x] `ProjectsScreen` presentational — group by `projectId`, host rows, create form, per-project "Add to host" host selector (`tddy-web/src/components/projects/ProjectsScreen.tsx`)
- [x] Remove **create-project toggle + form** (and now-unused state/handler) from `ConnectionScreen`. NOTE: the session-listing accordions (`project-accordion-*`/`sessions-table-*`) are the old sessions view and are covered by 41 ConnectionScreen tests — they were **kept** (removing them was out of scope and would break those tests). (`tddy-web/src/components/ConnectionScreen.tsx`)
- [x] Update existing `list_projects_multi_daemon_aggregation.rs` for the new `local_only` field (`ListProjectsRequest { .., local_only: false }`)

## Acceptance tests

- [x] `packages/tddy-web/cypress/component/ProjectsScreenAcceptance.cy.tsx` — 4/4 passing

## Unit / integration tests

- [x] `packages/tddy-daemon/tests/add_project_to_host_acceptance.rs` — 3/3 passing
- [x] `packages/tddy-daemon/tests/list_projects_local_only.rs` — 2/2 passing
- [x] `packages/tddy-daemon/tests/project_storage_add_or_get.rs` — 2/2 passing
- [x] `classify_peer_route` — pre-existing coverage in `livekit_peer_discovery.rs` `#[cfg(test)]` (reused as-is)

## Status: GREEN

Implementation complete; the tests above pass. Verified:
`cargo build -p tddy-daemon` clean, `cargo clippy -p tddy-daemon -- -D warnings`
clean, `tddy-web` vite build clean, `ProjectsScreenAcceptance.cy.tsx` 4/4,
plus regression checks (`ConnectionScreen.cy.tsx` 41/41; `appRoutes` +
`ConnectionScreen` bun unit tests 32/32).

**Scope note:** the plan called to remove the "Projects table" (`ConnectionScreen`
~lines 2051–2290), but that range is the **session-listing accordions**
(`project-accordion-*`/`sessions-table-*`) — the core of the old sessions view,
covered by 41 tests. Only the **create-project toggle + form** (genuinely relocated
to `/projects`) and its now-unused state/handler were removed; the session
accordions were kept. Fully retiring the old sessions view is a separate change.

**LiveKit fan-out note:** `LiveKitEligibleDaemonSource::peer_project_entries` is
exercised in production (not by these tests, which use a fake `EligibleDaemonSource`);
it bridges to the async `forward_to_peer` via `block_in_place` + `Handle::block_on`,
guarded for the no-runtime case.

## Final verification (pr-wrap, 2026-07-03)

- `tddy-daemon` full package suite: **212 passing, 0 failing** (after building
  `tddy-sandbox-runner`, which `sandbox_session` unit tests require as a precondition).
- `cargo clippy -p tddy-daemon -- -D warnings` clean; `cargo fmt --check` clean.
- `tddy-web` vite build clean; `ProjectsScreenAcceptance.cy.tsx` 4/4; regression
  `ConnectionScreen.cy.tsx` 41/41; bun units 32/32.
- Adding `room_slot` to `LiveKitEligibleDaemonSource::new` required updating 4 existing
  callers (`livekit_peer_daemons_acceptance.rs`, `multi_host_acceptance.rs`,
  `relay_e2e_acceptance.rs`, and the `livekit_peer_discovery.rs` lib test) — done.

**Pre-existing, unrelated to this feature (present on `origin/master`):** full
`./test` fails to compile `tddy-sandbox-darwin`'s `sandbox_runner_acceptance.rs`
(`SandboxRunnerArgs` missing `append_system_prompt_file`, introduced by #264). Not
touched here; flagged for a separate fix.

## Validation Results (pr-wrap, 2026-07-03)

Scope validated: the uncommitted feature delta only (`git diff HEAD` + untracked) —
the branch also carries 2 pre-existing committed workflow-orchestration commits that
are **not** part of this feature.

**Risk: Critical 0 · Warning 0 · Info 3.**

- **validate-changes**: `add_project_to_host` mirrors `create_project` (clone) +
  `start_session` (routing) exactly; auth/validation/idempotency correct;
  `add_or_get_project` idempotency consistent with the existing read-then-write
  pattern. `list_projects` `local_only` short-circuit is correct. No dangling
  create-project refs left in `ConnectionScreen`.
- **validate-tests**: 4 red tests are fluent-style (Given/When/Then, named helpers,
  exact assertions, one behavior each); Rust clone test hermetic via absolute
  `repos_base_path`. One fixture lifetime fix applied by the implementer (source-repo
  tempdir kept alive) — not a structural weakening.
- **validate-prod-ready**: no `red/green phase` strings, no stray TODO/FIXME, no new
  `unwrap`/`expect`/`panic!` in production, no stdout/stderr in production paths.
- **analyze-clean-code**: small focused functions, clear names, presentational/container
  split on the web side.

**Info notes (non-blocking):**
1. `packages/tddy-web/src/buildId.ts` changed — auto-generated build-time artifact (noise).
2. `peer_project_entries` fans out to peers **serially** and bridges the sync trait
   method to async via `block_in_place` + `Handle::block_on` — valid on the daemon's
   `new_multi_thread` runtime; tests use a fake source so this LiveKit path is
   production-only (no automated coverage).
3. The idempotent handler calls `find_project` before `add_or_get_project` (which
   re-checks) — a harmless redundant read.

## Delta summary (planned)

### `tddy-service`

- `proto/connection.proto`: new `AddProjectToHost` RPC on `ConnectionService`;
  `AddProjectToHostRequest { session_token, project_id, name, git_url,
  main_branch_ref, daemon_instance_id, user_relative_path }`;
  `AddProjectToHostResponse { ProjectEntry project }`; `ListProjectsRequest`
  gains `bool local_only`.

### `tddy-daemon`

- `project_storage.rs`: `add_or_get_project(projects_dir, ProjectData) ->
  (ProjectData, bool /*created*/)`.
- `livekit_peer_discovery.rs`: generalize routing into `classify_peer_route`;
  add `forward_add_project_to_host_via_livekit`; implement
  `LiveKitEligibleDaemonSource::peer_project_entries` (fan-out with
  `local_only=true`).
- `connection_service.rs`: `add_project_to_host` handler (mirrors `create_project`
  clone path + `start_session` routing, reuses given `project_id`, idempotent);
  `list_projects` honors `local_only`.

### `tddy-web`

- `routing/appRoutes.ts`: `PROJECTS_ROUTE` + `isProjectsPath`.
- `index.tsx`: `/projects` dispatch branch.
- `components/shell/DaemonNavMenu.tsx`: Projects nav item.
- `components/projects/ProjectsAppPage.tsx` (new, container).
- `components/projects/ProjectsScreen.tsx` (new, presentational).
- `components/ConnectionScreen.tsx`: remove Projects section + create-project form.
- `cypress/support/pages/projectsScreenPage.ts` (new page object) + new test IDs.
