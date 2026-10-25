# Changeset: list and reach sessions on every host

**Date**: 2026-10-25
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — three crates plus a proto change (exec_tools.proto), a cross-daemon trust decision (delegated caller), multi-daemon integration tests
**Stack**: `#start-parity` 1/9 — branch `feature/start-parity/remote-targets`, base `master`
**PRD**: [PRD-2026-10-25-start-parity-remote-targets.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-remote-targets.md)

## Initial Discovery

See [./2026-10-25-start-parity-remote-targets-initial-discovery.md](./2026-10-25-start-parity-remote-targets-initial-discovery.md).

## Prerequisites

- ℹ [2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md](../todo/2026-10-24-peer-forwarding-tests-use-livekit-below-the-binary-boundary.md) — if the in-memory `PeerForwarder` testkit lands first, these tests use it; otherwise they use the LiveKit testkit.

## Responsibility

- The fan-out list and the `daemon_instance_id` on `ListedSession`.
- The `daemon_instance_id` argument on every project-home target tool, and the forward of the
  whole call to the target's host.
- The delegated home caller: its proto, how the forwarding host fills it in, and how a peer
  accepts it.
- Read, diff and one git operation proven across hosts. This node covers what were planned as the
  `remote-read-diff` and `remote-git` nodes.

## Boundaries

- Does not start sessions anywhere (that is `start-tool`) and adds no new tool.
- Does not forward the worktree, activity or terminate tools. Those nodes add their tools and each
  proves its own cross-host case through this node's forward.
- Does not change `ListSessions` (the RPC) or how the web fans out.

## Dependencies

None — this node is a root of the dependency graph. It still sits on its line predecessor's branch
(the flattening), but consumes nothing from it.

## Draft PR contract

The **first push of this PR**, not its deliverable — this PR goes on to implement all of it:

- `exec_tools.proto`: `message DelegatedHomeCaller { string home_session_id; string project_id;
  string home_daemon_instance_id; }`, plus `ExecuteToolRequest.delegated_home_caller`.
- `tddy-tool-engine`: `ListedSession.daemon_instance_id`, `ListProjectSessionsResult
  { sessions, unreachable_hosts }`, and the `daemon_instance_id` property on every target tool's
  schema.
- `tddy-worktree-service::session_target`: `HomeCaller::delegated(...)`.
- Failing acceptance tests (below).

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — its tests need only its own surface and today's master
**Concurrent with:** `n2` `start-tool` (`feature/start-parity/start-tool`)
**Blocks:** `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`)

Real dependency edges (the branch line is only their linear flattening):

    n1 → n3, n5, n6, n7      n2 → n4, n9      n3 → n8      n4 → n9

Line: `n1 remote-targets · n2 start-tool | n3 worktree-list · n4 async-start · n5 start-options ·
n6 session-follow · n7 terminate | n8 disk-refresh · n9 spawn-conversation`, waves `1 1 2 2 2 2 2 3 3`.

## Successor PRs

`n2` `start-tool` (`feature/start-parity/start-tool`), `n3` `worktree-list` (`feature/start-parity/worktree-list`), `n4` `async-start` (`feature/start-parity/async-start`), `n5` `start-options` (`feature/start-parity/start-options`), `n6` `session-follow` (`feature/start-parity/session-follow`), `n7` `terminate` (`feature/start-parity/terminate`), `n8` `disk-refresh` (`feature/start-parity/disk-refresh`), `n9` `spawn-conversation` (`feature/start-parity/spawn-conversation`)

## Affected Packages

- [ ] `tddy-service (exec_tools.proto)`
- [ ] `tddy-daemon-rpc`
- [ ] `tddy-tool-engine`
- [ ] `tddy-worktree-service`

## Related Feature Documentation

- PRD: [PRD-2026-10-25-start-parity-remote-targets.md](../../ft/daemon/1-WIP/PRD-2026-10-25-start-parity-remote-targets.md)
- [docs/ft/daemon/project-concept.md § Project-home tools](../../ft/daemon/project-concept.md)

## Summary

A project-home session can only see and act on sessions of its own daemon. Its project's
sessions on other registered hosts are invisible to `ListProjectSessions`, and every target tool
(`ReadSessionWorktree`, `DiffSessionWorktree`, the seven git tools) refuses them. This node makes
the orchestrator's reach span every eligible host: the list fans out across hosts, and every
project-home tool call that names a target on another host is forwarded to that host and run
there.

## Scope

- [ ] `ListProjectSessions` lists the project's sessions on every eligible host, each with its host.
- [ ] An unreachable host is reported, not silently omitted.
- [ ] Read, diff and every git tool work on a session on another host.
- [ ] A target outside the orchestrator's project is refused on whichever host holds it.
- [ ] A delegated caller is honoured only from a peer daemon over the common room.

## Technical Changes

### State A

- `ListProjectSessions` lists only `list_sessions_at_session_coordinate` on this daemon
  (`tddy-daemon-rpc/src/exec_tool/project_home.rs:264-296`). `ListedSession` has no host.
- `resolve_session_target` (`tddy-worktree-service/src/session_target.rs:69-98`) is pure over
  this daemon's `TargetRecord`s.
- `ExecuteTool` already forwards a whole call on `ExecuteToolRequest.daemon_instance_id`
  (`exec_tool/ports.rs:42-53`, through `PeerRouting::rpc_served_by_peer`). On the peer,
  `project_home_caller` reads the home session's `.session.yaml` **locally** to check its role,
  finds no file, and refuses with PERMISSION_DENIED.

### State B

- `ListProjectSessions` asks every eligible daemon (the roster `ListEligibleDaemons` serves)
  for the project's sessions and merges the answers. Each row carries `daemon_instance_id`, and a
  host that does not answer is reported in an `unreachable_hosts` list, never silently dropped.
- Every target tool's arguments gain an optional `daemon_instance_id` (empty = this host). A call
  naming another host is forwarded to it as an `ExecuteToolRequest`, carrying a **delegated home
  caller**: the home session id, its project and the home daemon's id.
- The peer serves a project-home tool for a delegated caller without reading the home session's
  record. It still checks that the target is in the asserted project and that the caller's token
  maps to the same OS user, and it applies its own path containment, activity gate and push identity.
- The delegated caller is accepted **only** on a request that arrived over the common room from the
  daemon it names. A delegated caller in a request arriving over `/rpc` or IPC is refused.

## Testing Plan

### Acceptance Tests

- [ ] **Integration** `packages/tddy-daemon/tests/project_home_remote_targets_acceptance.rs` — `list_project_sessions_reports_the_projects_sessions_on_every_host_with_their_host`: Two daemons with one session each in the project. The list holds both, each with the right `daemon_instance_id`.
- [ ] **Integration** `same file` — `a_host_that_does_not_answer_is_named_not_dropped`: Peer forwarder that fails. The result names the peer in `unreachable_hosts`, and the local sessions are still listed.
- [ ] **Integration** `same file` — `read_session_worktree_reads_a_file_from_a_session_on_another_host`: Content equals the file written in the peer's worktree.
- [ ] **Integration** `same file` — `diff_session_worktree_diffs_a_session_on_another_host`: The diff contains the peer worktree's uncommitted change.
- [ ] **Integration** `same file` — `commit_session_worktree_commits_on_the_target_host_under_that_hosts_identity`: Peer HEAD advances; the author is the peer's resolved identity.
- [ ] **Integration** `same file` — `a_target_outside_the_asserted_project_is_refused_on_the_peer`: Error names "another project"; nothing is read.
- [ ] **Integration** `same file` — `a_delegated_caller_arriving_over_rpc_is_refused`: A direct `/rpc` ExecuteTool carrying `delegated_home_caller` is PERMISSION_DENIED.

Tests are written in the `fluent-tests` style (Given / When / Then). Multi-daemon tests use the
in-memory peer forwarder if it exists on the base, and the LiveKit testkit otherwise.

## Technical Debt & Production Readiness

## Decisions & Trade-offs

- **One generic forward, not one forward per tool.** Each tool runs on the host that holds the
  target, unchanged, so its containment, gate and identity rules are that host's rules. This is
  why `remote-read-diff` and `remote-git` were folded in.
- **Trust.** The peer accepts the asserted caller only from the daemon named in it, over the common
  room — the same trust a forwarded `StartSession` already gets. Score 5 because this is a new
  trust edge, and the developer should confirm it at the acceptance-test review.

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

## TODO

- [x] Record initial discovery (`2026-10-25-start-parity-remote-targets-initial-discovery.md`)
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
