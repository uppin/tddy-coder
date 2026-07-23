# Changeset: project-default-branch — default branch as a project property + unified resolution

**Date:** 2026-07-23
**Branch:** `feat-default-branch`
**Packages:** `tddy-service` (proto), `tddy-core`, `tddy-daemon`, `tddy-web`
**Feature PRD:**
- [docs/ft/coder/git-integration-base-ref.md](../../ft/coder/git-integration-base-ref.md) — unified resolution, legacy-only probe, validation
- [docs/ft/web/projects-screen-multi-host.md § Default branch](../../ft/web/projects-screen-multi-host.md#default-branch) — dropdown UI

## Problem

The default integration base ref (origin/master vs origin/main) is resolved in two disconnected
places with different rules:

- **gRPC `StartSession`** (web) never consults the project registry — it relies on the client
  sending `selected_integration_base_ref`, else the live `resolve_default_integration_base_ref`
  probe in tddy-core.
- **Telegram** consults `effective_integration_base_ref_for_project`, whose legacy fallback is a
  **hardcoded `origin/master`** (constant `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`), not the live
  probe.

The project already stores `main_branch_ref`, but there is no way to read or set it from the UI
(`ProjectEntry` has no such field; there is no update RPC). Result: the default branch is neither
authoritative nor user-controllable.

## Goal

Make the default branch a first-class, user-settable **property of the project**, and unify all
default-base resolution behind one resolver. The live origin/master→origin/main→origin/HEAD probe
becomes **legacy-only**: it resolves the default for projects without a stored branch and loses
effect once the branch is set.

## Design

1. **Unified resolver (`tddy-daemon` `project_storage`).**
   `effective_integration_base_ref_for_project(projects_dir, project_id)`:
   - `main_branch_ref` set → return it (validated), no probe.
   - unset (legacy) → `tddy_core::resolve_default_integration_base_ref(&project.main_repo_path)`
     (live probe) instead of the hardcoded `DOCUMENTED_DEFAULT_INTEGRATION_BASE_REF`.

2. **Relaxed validation.** Project default branch is validated with
   `validate_chain_pr_integration_base_ref` (accepts multi-segment `origin/<path>`), so any remote
   branch from `list_recent_remote_branches` is a legal default. `validate_integration_base_ref`
   (single-segment) still governs the per-session `selected_integration_base_ref` override.

3. **Storage mutator.** New `set_project_default_branch(projects_dir, project_id, ref)`: validates,
   updates the matching row, persists; rejects invalid ref before any write; errors on unknown id.

4. **Proto + RPC (`tddy-service` `connection.proto`).**
   - `ProjectEntry.main_branch_ref = 6` (empty = unset). Populated in `list_projects`,
     `create_project`, `add_project_to_host` mappings.
   - New `SetProjectDefaultBranch(SetProjectDefaultBranchRequest{session_token, project_id,
     main_branch_ref, daemon_instance_id}) → SetProjectDefaultBranchResponse{project}`. Persists via
     `set_project_default_branch`, then forwards to peer hosts owning the same `project_id`
     (logical-project scope, mirroring `AddProjectToHost` peer forwarding).

5. **StartSession unification (`tddy-daemon`).** When `req.selected_integration_base_ref` is empty,
   resolve the base through `effective_integration_base_ref_for_project` and thread it as the
   worktree base (via `setup_worktree_for_session_with_optional_chain_base`, which accepts
   multi-segment refs).

6. **Projects UI (`tddy-web`).** Per-project default-branch `<select>` in `ProjectCard`. Loads
   branches via `listProjectBranches` for the project's first host; shows the stored
   `mainBranchRef` selected, else pre-selects `origin/master` (else `origin/main`); onChange calls
   `setProjectDefaultBranch` and refreshes.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] `ProjectEntry.main_branch_ref` + `SetProjectDefaultBranch` RPC/messages in `connection.proto`; regenerate Rust + TS bindings
- [x] Relax project-default validation to `validate_chain_pr_integration_base_ref` in `project_storage` (`add_project`, resolver, mutator)
- [x] Unified `effective_integration_base_ref_for_project` (legacy → live probe) in `project_storage`
- [x] `set_project_default_branch` storage mutator in `project_storage`
- [x] `set_project_default_branch` RPC handler + peer forward in `connection_service.rs` (+ tonic adapter)
- [x] Populate `main_branch_ref` in `ProjectEntry` mappings (`list_projects`, `create_project`, `add_project_to_host`)
- [x] `StartSession` empty-override → use the project's stored default (claude-cli + both sandboxed helpers); legacy falls through to the live probe
- [x] `ProjectsScreen`/`ProjectCard` default-branch dropdown + `ProjectsAppPage` RPC wiring (`setProjectDefaultBranch`, branch loader)
- [x] Test-ids + `projectsScreenPage` helpers for the default-branch selector

## Acceptance tests

- [x] `packages/tddy-daemon/tests/project_default_branch_resolution_acceptance.rs` — 4/4 pass
- [x] `packages/tddy-daemon/tests/set_project_default_branch_acceptance.rs` — 4/4 pass
- [x] `packages/tddy-web/cypress/component/ProjectsScreenAcceptance.cy.tsx` — 12/12 pass (7 existing + 5 new)

## Unit tests

- [x] `packages/tddy-daemon/src/project_storage.rs` (`set_project_default_branch`, relaxed validation, unified resolver) — pass

## Implementation status (2026-07-23)

Implemented and green. Regression check: existing `add_project_to_host`, `list_projects_*`,
`non_blocking_peer_fanout`, `claude_cli_session`, and `multi_host` acceptance suites pass. The one
red test, `cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`,
fails only on this host's unprivileged-user-namespace restriction (cgroups sandbox needs root) and
is unrelated to this change.

## Validation Results (2026-07-23)

- **validate-changes:** 0 critical / 0 warning. Only non-test `unwrap`-family call is
  `main_branch_ref.clone().unwrap_or_default()` (safe); all others are in `#[cfg(test)]` helpers.
  No fallbacks, no test-only branches, no `unsafe`. New RPC handler validates before mutation and
  maps `InvalidArgument`/`NotFound`/`FailedPrecondition` precisely. Code aligns with the PRD.
- **validate-tests:** Fluent Given/When/Then, named builders (`a_source_repo_with_branches`,
  `a_clone`, `aProjectsBackend`), one behavior per test, exact assertions, no branching/try-catch.
- **validate-prod-ready:** No `TODO`/`FIXME`/mock/hardcoded values in production paths. Peer-forward
  for `SetProjectDefaultBranch` is fully wired (no stub).
- **analyze-clean-code:** Clean. Minor (info): the peer-routing preamble is duplicated between
  `add_project_to_host` and `set_project_default_branch`; extracting a shared helper is a possible
  future tidy but would touch existing code and is out of scope here.
- **Lint/build:** `cargo fmt` applied; `cargo clippy -p tddy-daemon -p tddy-core -- -D warnings`
  clean (exit 0). Feature + regression suites green (see Implementation status).

## Out of scope / follow-ups

- Cross-host peer-forward propagation has an acceptance test only at the local-registry read-back
  level in this changeset; a multi-daemon forward acceptance (stub peers) mirrors
  `add_project_to_host` forwarding and is a follow-up.
