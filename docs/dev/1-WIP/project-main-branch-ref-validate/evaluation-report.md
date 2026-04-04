# Evaluation Report

## Summary

Branch adds integration-base ref validation, fetch/worktree helpers, project registry resolution (effective_integration_base_ref_for_project, add_project validation), and heuristic default-branch resolution in setup_worktree_for_session. ./dev cargo check --workspace passed. Main gap: project main_branch_ref is not threaded into daemon_service or TDD hooks; worktree creation still uses the two-arg setup_worktree_for_session. The integration test worktree_uses_configured_project_base_ref exercises a main-only remote via resolve_default_integration_base_ref, not a persisted projects.yaml project setting.

## Risk Level

medium

## Changed Files

- packages/tddy-core/src/lib.rs (modified, +4/−2)
- packages/tddy-core/src/worktree.rs (modified, +298/−9)
- packages/tddy-daemon/src/connection_service.rs (modified, +1/−0)
- packages/tddy-daemon/src/project_storage.rs (modified, +96/−0)
- packages/tddy-daemon/tests/acceptance_daemon.rs (modified, +1/−0)
- packages/tddy-daemon/tests/multi_host_acceptance.rs (modified, +1/−0)
- packages/tddy-integration-tests/tests/worktree_acceptance.rs (modified, +93/−0)

## Affected Tests

- packages/tddy-core/src/worktree.rs: updated
  integration_base_red_tests (fetch_integration_base, setup_worktree_with_integration_base)
- packages/tddy-daemon/src/project_storage.rs: updated
  project_integration_base_acceptance_tests (legacy default, invalid ref)
- packages/tddy-daemon/tests/acceptance_daemon.rs: updated
  acceptance_project_storage_roundtrip ProjectData extended
- packages/tddy-daemon/tests/multi_host_acceptance.rs: updated
  per_host_project_path_roundtrip ProjectData extended
- packages/tddy-integration-tests/tests/worktree_acceptance.rs: updated
  Added worktree_uses_configured_project_base_ref

## Validity Assessment

Partially valid versus the PRD. Delivers validation, storage, legacy default resolution for projects.yaml, and better default worktree behavior for main-only remotes when no project context exists. Missing: threading main_branch_ref into daemon and workflow session worktree setup, proto/RPC/UI beyond main_branch_ref: None in connection_service, and an end-to-end test that proves projects.yaml drives worktree HEAD. Build succeeded.

## Build Results

- workspace: pass (./dev cargo check --workspace (full workspace compile))

## Issues

- [high/prd-gap] packages/tddy-service/src/daemon_service.rs: PRD requires using per-project main_branch_ref when a session is tied to a project; daemon and tdd/hooks still call setup_worktree_for_session(repo, session_dir) only. effective_integration_base_ref_for_project exists but is not used on the worktree creation path.
- [medium/behavior-change] packages/tddy-core/src/worktree.rs: setup_worktree_for_session now resolves origin/master vs origin/main vs origin/HEAD after fetch instead of always using origin/master. Intended for main-only remotes but changes semantics for sessions without an explicit project ref.
- [low/performance] packages/tddy-core/src/worktree.rs: setup_worktree_for_session runs git fetch origin in resolve_default_integration_base_ref then fetch_integration_base issues a second fetch for the branch.
- [low/test-coverage] packages/tddy-integration-tests/tests/worktree_acceptance.rs: worktree_uses_configured_project_base_ref does not configure ProjectData or projects.yaml; it relies on heuristic resolution matching a main-only fixture.
- [low/workspace-hygiene] .: Untracked .red-phase-feature-tests-output.txt and .red-phase-submit.json present; should not be committed.
