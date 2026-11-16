# Changeset: The files and functions the #remote-shell stack grew are split back under their budgets

**Date**: 2026-11-16
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 5 — six packages, an inherited restructure across files four earlier stacks declined to split, one hand split in TypeScript, engine refusals already on record for two of the seams
**Branch**: `feature/remote-shell/sweep`, base `feature/remote-shell/coder-hosts` (#921) — `#remote-shell 8/8`

## Initial Discovery

Full collection that grounded this plan: [initial-discovery.md](./2026-11-16-remote-shell-sweep-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

No code issue in a touched package carries `Claimed by:` from another stack.

### ✅ RESOLVED HERE — `tddy-accounts` `service.rs` is past the 500-line budget — [`2026-10-21-tddy-accounts-service-rs-over-file-budget.md`](../todo/2026-10-21-tddy-accounts-service-rs-over-file-budget.md)

That entry names the split and waits for a moment when no open stack modifies the file. Nothing above this PR does. Seam group D performs the split it describes; the entry's file is deleted at this PR's wrap.

### ✅ RESOLVED HERE — the shared stub writers are not warmed — [`2026-11-16-shared-stub-writers-fixture-warmup.md`](../todo/2026-11-16-shared-stub-writers-fixture-warmup.md)

Written by #920's wrap. The warm-up is a test-support change with no production effect; seam group G adds it to `agent_registry_support` and `harness_clis_support` beside the fixtures #920 already warmed.

### ⚠ DURING — Coder hosts were never run against a real deployment — [`2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)

Not closable by a restructure: it needs a Coder deployment. This PR moves `remote_agent_launch.rs` and must not change what the stand-ins pin.

## Affected Packages

- **tddy-agent-launch**: `remote_agent_launch.rs` becomes a module with `startup` and `target` submodules; the target and Coder wiring leaves `start_cli_harness_session`, `resume_cli_harness_session` and `resume_cli_harness`. `docs/` module-layout pages name the new modules.
- **tddy-session-lifecycle**: `svc_activity_ports.rs`, `svc_host_builders.rs` and `connection_service.rs` each lose a group of items to a submodule. `docs/` layout pages follow.
- **tddy-accounts**: `service.rs` loses its provider-account half and its link half to modules beside `provider_account.rs` and `linking.rs`.
- **tddy-worktree-service**: `remote_git_service.rs` loses request resolution and the child relay to two files.
- **tddy-daemon**: `runtime/mount_host.rs` `build_session_host` loses the Coder credential wiring to a helper.
- **tddy-session-create-web**: `useCreateSessionPaneState.tsx` loses the derived-host resolution to a hook of its own. Hand split.
- **tddy-session-lifecycle / tddy-agent-launch tests**: `agent_registry_support`, `harness_clis_support` gain the fixture warm-up.

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

Every code-issue record the `#remote-shell` stack added or made worse is closed, or brought back to its measurement before the stack. Nothing changes behaviour. Every moved Rust item is moved by `tddy-tools restructure`; the one TypeScript file is split by hand under its existing specs. Every path another crate or a test names stays reachable.

## Background

Each PR of the stack was wrapped while a later PR still modified the same files, and a wrap may not split a file a successor touches: the move would arrive in the successor as a conflict over code it is still writing. So seven wraps recorded what they grew and left it. This PR is above all of them, and nothing is above it.

Two of the files were over budget long before this stack (`remote_git_service.rs` since 2026-09-23, `tddy-accounts` `service.rs` since 2026-10-21) and were deferred by earlier stacks for the same reason. The stack made both worse, so they are in scope, and their recorded designs are what this PR applies.

## Responsibility

This PR owns:

- Closing or restoring the sixteen records listed in State A, by the seams in the Delta.
- The two backlog entries marked ✅ RESOLVED HERE.
- Re-measuring every record it touches and deleting each one it closes, with the final measurement recorded in this PR's change-history entry first.

## Boundaries

This PR does not:

- change behaviour, a signature another crate calls, a proto, or a test assertion;
- write moved Rust code by hand. A seam the engine refuses is a stop, reported with the refusal, never a hand move;
- add a code-issue record or a backlog entry of its own;
- split generated output (`gen/*_pb.ts`);
- deduplicate the test fixtures the stack's test files repeat. That is recorded for the developer's decision, not done here;
- touch the four records the stack re-measured unchanged (`wire_session_agent`, `resumed_session_wiring`, `slots::render`, `unlock_and_add`).

## Dependencies

Every PR of the stack is beneath this one and is inherited as it stands: #915 `shell-port`, #916 `coder-account`, #917 `ssh-agent-launch`, #918 `coder-shell`, #919 `derived-hosts`, #920 `remote-hooks`, #921 `coder-hosts`. All seven are implemented, wrapped, green and frozen. This PR consumes their code as the subject of its moves and changes none of their behaviour.

## Draft PR contract

The first push after the plan (commit 2) publishes the recorded baseline and the `plan.jsonl` files, each proven with `restructure check --deep`. A restructure has no failing tests: the baseline is the contract. The same PR then applies the plans.

## Green wave

**Wave:** 5 of 5
**Greenable independently:** after every PR below it. It moves their code.
**Concurrent with:** none
**Blocks:** none

    n1 … n7 → n8

## Scope

- [ ] **Plan**: seam order fixed, anchors emitted by the tool, every plan passes `check --deep`
- [ ] **Apply**: each plan applied and committed on its own, `verify --against` clean
- [ ] **Baseline**: every touched package's suite back to the recorded failing set, by name
- [ ] **Code Quality**: clippy `-D warnings` and fmt clean on every touched package; the TypeScript package's specs and typecheck as before
- [ ] **Records**: each closed record deleted with its final measurement in the change-history entry; each restored record re-measured
- [ ] **Documentation**: Final Checklist derived

## Technical Changes

### State A

| # | File or function | Before the stack | Now | Budget |
|---|---|---|---|---|
| 1 | `tddy-agent-launch/src/remote_agent_launch.rs` | new | 579 | 500 |
| 2 | `tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs` | 513 | 546 | 500 |
| 3 | `tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` | 531 | 544 | 500 |
| 4 | `tddy-session-lifecycle/src/connection_service.rs` | over 500 | +6 | 500 |
| 5 | `tddy-accounts/src/service.rs` | 772 | 790 | 500 |
| 6 | `tddy-worktree-service/src/remote_git_service.rs` | 1,004 | 1,111 | 500 |
| 7 | `tddy-session-create-web/src/useCreateSessionPaneState.tsx` | 495 | 503 | 500 |
| 8 | `tddy-rust-typescript-tests/gen/host_pb.ts` (generated) | 1,402 | 1,734 | 500 |
| 9 | `start_cli_harness_session` | 319 | 368 | 60 |
| 10 | `resume_cli_harness_session` | 157 | 166 | 60 |
| 11 | `resume_cli_harness` | 95 | 108 | 60 |
| 12 | `build_session_host` (`tddy-daemon`) | 73 | 82 | 60 |
| 13 | `start_cli_harness_from_request` | 98 | 100 | 60 |
| 14 | `start_sandboxed_cli_harness_session` | 291 | 292 | 60 |
| 15 | `start_session_on_request` | 79 | 80 | 60 |
| 16 | `DaemonSessionHost::new` | 97 | 100 | 60 |

Lines are production lines, counted to the first `#[cfg(test)]`; function lengths are brace-matched, as the records measure them.

### State B

| # | Target | How |
|---|---|---|
| 1 | under 500 | `remote_agent_launch/startup.rs` (the startup wait, the window, the settled output) and `remote_agent_launch/target.rs` (the target resolution and the two proto conversions) |
| 2 | under 500 | the hook delivery this stack added, and enough of its neighbours, into `svc_activity_ports/hook_delivery.rs` |
| 3 | under 500 | the host stores and the default-socket builders the `#agent-tools` sweep left beside `new`, and this stack's Coder builder, into `svc_host_builders/stores.rs` |
| 4 | at or below its length before the stack | the re-export block this stack lengthened moves with its neighbours into a module the facade re-exports |
| 5 | under 500 | the provider-account half beside `provider_account.rs`, the link half beside `linking.rs`, as the backlog entry designs |
| 6 | under 500 | seam A (request resolution and argv) and seam B (the child relay and its pumps, with this stack's streamed path) into two files, as the record designs |
| 7 | under 500 | the derived-host resolution into `useDerivedHostSelection.ts`, by hand |
| 8 | unchanged | no seam exists for generated output; see Decisions |
| 9–11 | at or below the length before the stack | `extract_method` of the target resolution and Coder credential wiring each gained |
| 12 | at or below 73 | `extract_method` of the Coder credential wiring |
| 13–16 | at or below the length before the stack where the engine offers a seam | `extract_method`; a refusal is recorded, see Decisions |

A record whose file or function ends at or under its budget is **closed** and deleted. One that ends at or below its measurement before the stack, and still over budget, is **restored**: the stack no longer made it worse, and it stays open as the debt it was.

### Delta — seam groups in dependency order

1. **A — `tddy-agent-launch` file** (row 1): `extract_module` of the startup items, then of the target items. Definitions before users.
2. **B — `tddy-agent-launch` functions** (rows 9, 10, 11, 13, 14): `extract_method`, last range first within each function. After A, because A moves the callees.
3. **C — `tddy-session-lifecycle`** (rows 2, 3, 4, 15, 16): `move_item` / `move_impl_members` per file, then `extract_method` for 15 and 16.
4. **D — `tddy-accounts`** (row 5): two `move_item` groups.
5. **E — `tddy-worktree-service`** (row 6): `extract_module --to_file`, seam A then seam B.
6. **F — `tddy-daemon`** (row 12): `extract_method`.
7. **G — test support**: the fixture warm-up in the two shared stub writers. Hand edit of test code, no production line.
8. **H — TypeScript** (row 7): hand split.

One plan file per group A–F, each committed on its own after its apply.

## Callers

| | |
|---|---|
| External importers | to be listed by the plan step from `restructure check --deep`: `tddy-daemon`, `tddy-session-lifecycle` and the test crates name items in rows 1, 5 and 6 |
| Facade | `reexport: "outside"` on every module with a consumer outside its crate, so no other crate's source changes |
| Consequence | no caller outside the moved crates is edited |

## Visibility

Filled in from each apply's report. Zero widenings is the target for groups D and E, whose seams are module-level items.

## Baseline

Recorded in the contract commit, before any change. The known failures on this machine that are not regressions, by cause: the LiveKit testkit's port bind, BSD `cat` rejecting `--` in the sandboxed-codebase suites, an unbuilt `tddy-remote-git-repo` binary, and the `/tmp` path assertion in `claude_cli_from_spec_acceptance`. CI on #921's head (`01a11360e`) is the reference for Linux: all 54 checks pass.

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-agent-launch` | recorded at the contract commit | |
| `./test -p tddy-session-lifecycle` | recorded at the contract commit | |
| `./test -p tddy-accounts` | recorded at the contract commit | |
| `./test -p tddy-worktree-service` | recorded at the contract commit | |
| `./test -p tddy-daemon` | recorded at the contract commit | |
| `tddy-session-create-web` specs and unit tests | recorded at the contract commit | |

## Plan

`docs/dev/1-WIP/2026-11-16-remote-shell-sweep/plan-<group>.jsonl`, one per group A–F, written with `restructure anchors` and proven with `check --deep` in the contract commit.

## Decisions & Trade-offs

- **"Closed or restored" is the bar.** A record the stack added is closed. A record that existed before is brought back to at most its measurement before the stack; where the designed split is on record and nothing above this PR blocks it (rows 5 and 6), it is closed outright.
- **Rows 13–16 grew by one to three lines.** The `#agent-tools` sweep scoped its function tier at five lines. They are attempted here because the bar is "not worse", but rows 12 and 16 carry recorded engine refusals for their remaining seams. A refusal there is recorded in the record with the engine's words, and the row is reported to the developer as not restored.
- **Row 8 has no seam.** `host_pb.ts` is generated from `host.proto`; the only way to shorten it is to split the proto's service, which is a wire change. Whether generated output should be measured against the file budget at all is the developer's call and is put to them when this PR is done. This PR leaves the record as it is.
- **Complexity 5 is a warning.** This is five packages' worth of moves in one PR. It is one concern, the stack's own debt, and each group is a separate plan and commit so a reviewer can take them one at a time. If the plan step shows a group cannot be proven, the group is cut and reported, not forced.
- **Fixture duplication is not moved here.** The stand-in target, stub agents and daemon helpers are repeated across four of the stack's test files. Moving them is a hand edit across test files owned by four PRs; it is recorded for the developer's decision.

## Final Checklist

Derived at the plan step: every statement in `packages/*/docs/` that names a moved item's file.

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

- [x] Record initial discovery (`2026-11-16-remote-shell-sweep-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [ ] Record the green baseline, by failing-test name
- [ ] Write the plans with `restructure anchors`; prove each with `check --deep`
- [ ] USER REVIEW — the plan
- [ ] Apply each plan, commit it on its own, `verify --against`
- [ ] Hand split of the TypeScript file; fixture warm-up in the shared stub writers
- [ ] Final gate: fmt, clippy, the baseline's failing set by name, the comment-line multiset, `verify` against the ref before the first plan
- [ ] Re-measure and close or restore every record
- [ ] Validate changes (/validate-changes)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
