# Changeset: An orchestrator reads every tddy worktree, agent home and chat file on its host

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — new reach resolver across three crates, a persisted per-session grant store, a containment rule changed on a security boundary, agent YAML slot
**Stack**: `#orchestra` 1/9 — branch `feature/orchestra/path-reach`, base `feature/target-parity/target-worktree-stats`

## Initial Discovery

[2026-11-21-path-reach-initial-discovery.md](./2026-11-21-path-reach-initial-discovery.md)

## Affected Packages

- **tddy-tool-engine**: `contain_path` / `tool_read` take a reach instead of a single worktree root; absolute paths accepted for orchestrator roles
- **tddy-daemon-rpc**: `exec_tool/role_guard.rs` + `project_home/caller.rs` build the caller's reach and hand it to the read tools
- **tddy-session-activity**: `PathGrantStore` — per-session grants, persisted in the session directory (empty until n6 writes one)
- **tddy-coding-agent-specs**: new `coding_agent.home_dirs` slot naming each agent's home / chat directories (e.g. `~/.claude`, `~/.cursor`, Muse's home)
- **tddy-projects**: read-only: lists every project's worktrees on the host

## Related Feature Documentation

- PRD: [PRD-2026-11-21-path-reach.md](../../ft/daemon/1-WIP/PRD-2026-11-21-path-reach.md)

## Summary

An orchestrator-role session's daemon `Read`, `Grep` and `Glob` accept absolute paths and answer for any path inside its **reach**: every worktree of every tddy project on its host (its own project's included), and the home and chat directories each installed coding agent declares in its YAML. A path outside reach is refused with a message naming the reach and pointing at `RequestPathAccess` (n6). Reach also includes the session's **grants**, read from a per-session store that this node creates and n6 fills.

## Responsibility

- `OrchestratorReach` (resolver: project worktrees + agent `home_dirs` + grants) and the read tools' use of it.
- `PathGrantStore` / `PathGrant { path, mode: Read|Write, granted_at, reason }`, persisted under the session directory; read API used here, write API used by n6.
- `coding_agent.home_dirs` in the agent spec schema and the shipped YAMLs (claude, cursor, muse, codex, opencode).

## Boundaries

- No elicitation, no tool that adds a grant (n6).
- No write access of any kind (n6).
- No LSP reach (n8).

## Dependencies

None inside `#orchestra`. It builds on its base, `#target-parity` 7/7 (#975), and on what is on master.

## Draft PR contract

The first push of this PR, before its implementation: `OrchestratorReach::{for_session, contains}`, `PathGrantStore::{open, grants, add}` and `PathGrant`, the `home_dirs` slot; failing tests below.

## Green wave

**Wave:** 1 of 4
**Greenable independently:** yes — needs nothing above #975
**Concurrent with:** n2, n3, n4, n5
**Blocks:** n6 (writes grants into this store), n8 (LSP roots default to this reach), n9

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7 · n8 · n9):

    n1 path-reach → n6 access-elicitation, n8 orchestrator-lsp
    n6 access-elicitation → n8 orchestrator-lsp
    n2 child-back-channel → n7 orchestrator-delivery
    n1 … n8 → n9 orchestrator-prompt

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7 · 3 = n8 · 4 = n9 (run `1 1 1 1 1 2 2 3 4`).
Inside wave 1, blockers lead: n1 (3 transitive dependents: n6, n8, n9), n2 (2: n7, n9), then n3, n4, n5
(1 each: n9) in foundational order. Inside wave 2: n6 (2: n8, n9) before n7 (1: n9). n3–n5 and n7 sit
in the line only because `gh stack` needs a line; none depends on its neighbour.

## Prerequisites

### ⚠ DURING — [2026-08-14-a-claude-cli-split-agent-has-no-route-to-its-own-attachments.md](../todo/2026-08-14-a-claude-cli-split-agent-has-no-route-to-its-own-attachments.md)

Same containment rule; this node widens it for orchestrator roles only. Not resolved here: split agents are not orchestrators.

### Unrelated — [2026-10-29-spawner-read-scope-is-same-host-only.md](../todo/2026-10-29-spawner-read-scope-is-same-host-only.md)

Reach is same-host by design here too; a cross-host reach would ride the existing forward and is not in this stack.

## Scope

- [ ] Reach resolver + absolute-path reads for orchestrator roles
- [ ] Per-session grant store (read side used here)
- [ ] `home_dirs` slot in agent specs and shipped YAMLs
- [ ] Feature doc `docs/ft/daemon/project-concept.md` § Orchestrator sessions updated through the wrap

## Technical Changes

### State A (Current)

`contain_path` (`packages/tddy-tool-engine/src/lib.rs:107`) refuses anything outside the caller's recorded worktree; `role_guard.rs:33-55` admits only read-only tools; agent specs carry no notion of a home directory.

### State B (Target)

Orchestrator-role reads are checked against `OrchestratorReach`; grants persist with the session.

## Acceptance Tests

`packages/tddy-daemon-rpc/tests/orchestrator_reach_acceptance.rs`:
- [ ] `an_orchestrator_reads_a_file_in_another_projects_worktree_by_absolute_path`
- [ ] `an_orchestrator_greps_across_a_sibling_sessions_worktree`
- [ ] `an_orchestrator_reads_a_file_under_an_agents_declared_home_dir`
- [ ] `a_path_outside_reach_is_refused_naming_request_path_access`
- [ ] `a_granted_path_is_readable_and_the_grant_survives_a_resume`
- [ ] `a_worker_sessions_reads_stay_confined_to_its_worktree`

`packages/tddy-coding-agent-specs/src (unit)`:
- [ ] `every_shipped_agent_declares_its_home_dirs`

## Decisions & Trade-offs

- Both orchestrator roles (`project-home` main and `orchestrator`) get this change: they share
  `is_orchestrator_role` and one wiring (developer, 2026-11-21).
- The stack sits on `#target-parity` 7/7 (#975), because that stack rewrites the remote hook path
  and answer files this stack builds on (developer, 2026-11-21).
- Enforcement is daemon-side, in the exec tools, not by giving Claude its native Read back — it holds for every harness (developer, 2026-11-21).
- Grants live for the session and are persisted in its directory (developer, 2026-11-21).

## Technical Debt & Production Readiness

_(populated during development)_

## Refactoring Needed

_(populated by validation)_

## Validation Results

_(populated by validation)_

## TODO

- [x] Record initial discovery (`2026-11-21-path-reach-initial-discovery.md`)
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
