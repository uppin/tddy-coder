# Changeset: The project-home tools can inspect and drive a split session

**Date**: 2026-11-15
**Status**: 🚧 In Progress
**Type**: Bug Fix
**Complexity**: 4 — three crates (`tddy-worktree-service`, `tddy-tool-engine`, `tddy-daemon-rpc`), a type change every target consumer sees, cross-host forwarding with two-daemon acceptance tests
**Stack**: `#split-alive` 3/4 — branch `feature/split-alive/split-targets`, base `feature/split-alive/split-hooks`
**PR**: _(set in pass 2)_

## Initial Discovery

[2026-11-15-split-session-targets-initial-discovery.md](./2026-11-15-split-session-targets-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-15-split-session-agent-not-running.md](../todo/2026-11-15-split-session-agent-not-running.md) | Referenced — claimed by `feature/split-alive/agent-liveness` (#905); this node closes its **fix 5** (§ C) |
| [packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md) | ⚠ **During** — the redirect lives in `project_home_remote.rs` beside `served_on_target_host`, not in `execute_tool` |
| `tddy-worktree-service` oversized-file records (`service.rs`, `worktrees.rs`, `remote_git_service.rs`) | — Unrelated (files not touched) |

## Affected Packages

- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — `SessionTarget.worktree` optional; `TargetRecord.codebase`
- **tddy-tool-engine**: [README.md](../../../packages/tddy-tool-engine/README.md) — worktree tools require the worktree explicitly; the worktree-tool list
- **tddy-daemon-rpc**: [README.md](../../../packages/tddy-daemon-rpc/README.md) — records carry the codebase half; worktree tools on a split agent are redirected

## Related Feature Documentation

- [PRD-2026-11-15-split-session-targets.md](../../ft/daemon/1-WIP/PRD-2026-11-15-split-session-targets.md)
- [project-concept.md § Project-home tools](../../ft/daemon/project-concept.md#project-home-tools)

## Summary

`resolve_session_target` stops refusing worktree-less records; the worktree is an `Option` on the
target and the worktree tools ask for it. Records carry their codebase half (from `SessionEntry`'s
`codebase_*`). The daemon redirects a worktree tool named with a split agent half's id to its codebase
half on the codebase host, re-using the existing target-host forward.

## Background

PRD § Background; discovery Exploration 2 § A.

## Responsibility

- `tddy-worktree-service/src/session_target.rs`: `SessionTarget.worktree: Option<PathBuf>`;
  `SessionTarget::worktree(&self) -> Result<&Path, SessionTargetError>` (`NoWorktree`);
  `pub struct CodebaseHalf { daemon_instance_id: String, session_id: String }`;
  `TargetRecord.codebase: Option<CodebaseHalf>` (also on `SessionTarget`); `resolve_session_target`
  no longer returns `NoWorktree`.
- Every worktree consumer switches to `target.worktree()?`: `project_home_tools.rs` (read, diff),
  `session_git_ops.rs` (`run_git_op`), `stack_git_tools/git_work.rs` (`target_for`, push, `worktree_for`).
- `tddy-tool-engine/src/project_home_target_host.rs`: `pub fn is_worktree_target_tool(name) -> bool`
  (read, diff, restore, every git tool) and `pub fn retarget_args(args_json, &CodebaseHalf) -> String`
  (sets `session_id` and `daemon_instance_id`).
- `tddy-daemon-rpc`: `port.rs` `target_records()` fills `codebase` from `row.entry.codebase_*`;
  `project_home_remote.rs` `served_on_target_host` — when the tool is a worktree tool, names no host,
  and the local record is a split agent half, forward to its codebase half with `retarget_args`.

## Boundaries

- Does **not** route agent-side tools named with a codebase half's id back to the agent half.
- Does **not** change the one-hop rule, admission, or the tools' argument schemas.
- Does **not** touch the start ledger, enrichment or web (`feature/split-alive/split-pairing`), nor
  liveness/hooks (#905, #906).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/split-alive/agent-liveness` (#905) | recording + startup watch | nothing (branch line only) | touch `tddy-session-split`, `tddy-pty`, `tddy-cli-sessions` |
| `feature/split-alive/split-hooks` (#906) | hooks flag | nothing (branch line only) | touch `tddy-coding-agent-specs` |

## Draft PR contract

First push (pass 2, commit 2): `CodebaseHalf`, `TargetRecord.codebase`, `SessionTarget.worktree:
Option<PathBuf>` with `worktree()` (implemented, since the type change must compile everywhere), the
resolver still refusing (`// TODO(split-targets): admit worktree-less targets`),
`is_worktree_target_tool` / `retarget_args` stubs, `target_records` still leaving `codebase: None`,
plus the failing tests below.

## Green wave

**Wave:** 1 of 1
**Greenable independently:** yes — tests use split records written to disk on the two-host harness and fake ports
**Concurrent with:** #905, #906, `feature/split-alive/split-pairing`
**Blocks:** none

Real dependency edges: none.

## Scope

- [ ] Agent-side tools resolve worktree-less sessions
- [ ] Worktree tools require the worktree explicitly and refuse as before when there is none and no codebase half
- [ ] Records carry the codebase half; worktree tools on a split agent half are redirected to it
- [ ] Docs: `project-concept.md` § Project-home tools states how a split session is targeted

## Technical Changes

### State A

Discovery Exploration 2 § A: `session_target.rs:18-98`; 12 `resolve_target` call sites; worktree
consumers listed in A.3; forwarding before resolution in `project_home_remote.rs:241-262`;
`target_records` at `port.rs:305-321` drops `codebase_*`.

### State B

As in § Responsibility. The redirect happens on the agent host, before resolution, so the codebase
host resolves its own workspace record exactly as it does today for an explicit
`daemon_instance_id` (the `diff_session_worktree_diffs_a_session_on_another_host` path).

### Delta

| Package | Change |
|---|---|
| `tddy-worktree-service` | `session_target.rs`, `session_git_ops.rs` |
| `tddy-tool-engine` | `project_home_tools.rs` (read/diff), `project_home_target_host.rs` |
| `tddy-daemon-rpc` | `project_home/port.rs`, `project_home_remote.rs`, `stack_git_tools/git_work.rs` |
| docs | `project-concept.md` |

## Implementation Milestones

- [ ] Type change compiles everywhere (contract commit)
- [ ] Resolver admits worktree-less targets; unit + fake-port tests green
- [ ] Records carry the codebase half; redirect; two-host acceptance green

## Testing Plan

Unit tests in `session_target.rs`; fake-port tests in `tddy-tool-engine/tests/`; two-daemon
acceptance in `tddy-daemon-rpc/tests/` on the existing `two_hosts()` harness (in-memory peer routing,
no LiveKit), with the split pair written as `.session.yaml` records on each host.

## Acceptance Tests

| Test | File | Validates |
|---|---|---|
| `a_split_agents_activity_is_read_where_the_agent_runs` | `packages/tddy-daemon-rpc/tests/project_home_split_targets_acceptance.rs` (new) | `ReadSessionActivity` on the agent half (no worktree) answers instead of refusing |
| `diffing_a_split_agent_diffs_its_codebase_on_the_codebase_host` | same | `DiffSessionWorktree{session_id: agent half}` with no host returns the codebase half's id and B's diff |
| `reading_a_split_agents_worktree_reads_the_codebase_host` | same | `ReadSessionWorktree` on the agent id lists B's worktree file |
| `a_session_with_no_worktree_and_no_codebase_is_still_refused_by_worktree_tools` | same | `DiffSessionWorktree` → "has no worktree" |

## Unit / Integration Tests

| Test | File |
|---|---|
| `resolving_a_session_without_a_worktree_targets_it`, `a_target_without_a_worktree_refuses_to_give_one`, `a_split_agent_record_names_its_codebase_half` | `packages/tddy-worktree-service/src/session_target.rs` (tests) |
| `reading_the_terminal_of_a_session_without_a_worktree_answers` | `packages/tddy-tool-engine/tests/project_home_terminal_tools.rs` |
| `worktree_tools_are_the_ones_that_need_a_checkout`, `retargeting_names_the_codebase_host_and_session` | `packages/tddy-tool-engine/tests/project_home_target_host.rs` |

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap docs

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Redirect in the daemon's forward, not in each tool**: one place, and the codebase host resolves
  its own record as it already does for an explicit host.
- **`worktree()` accessor over a separate target type**: keeps every port signature; the compiler finds
  each worktree consumer.

## Refactoring Needed

### From /validate-changes

### From /pr-wrap

## Validation Results

### /validate-changes

### /pr-wrap
