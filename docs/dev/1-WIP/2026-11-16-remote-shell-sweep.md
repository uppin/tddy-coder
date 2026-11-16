# Changeset: The files and functions the #remote-shell stack grew are split back under their budgets

**Date**: 2026-11-16
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 5 — six packages, an inherited restructure across files four earlier stacks declined to split, one hand split in TypeScript, engine refusals already on record for two of the seams
**Branch**: `feature/remote-shell/sweep`, base `feature/remote-shell/coder-hosts` (#921) — `#remote-shell 8/11`

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

- [x] **Plan**: seam order fixed, anchors emitted by the tool, every plan passes `check --deep`
- [x] **Apply**: each plan applied and committed on its own, `verify --against` clean
- [x] **Baseline**: every touched package's suite back to the recorded failing set, by name
- [x] **Code Quality**: clippy `-D warnings` and fmt clean on every touched package; the TypeScript package's specs and typecheck as before
- [ ] **Records**: each closed record deleted with its final measurement in the change-history entry; each restored record re-measured
- [ ] **Documentation**: Final Checklist derived

## Technical Changes

### State A

| # | File or function | Before the stack | Now | Budget |
|---|---|---|---|---|
| 1 | `tddy-agent-launch/src/remote_agent_launch.rs` | new | 579 | 500 |
| 2 | `tddy-session-lifecycle/src/connection_service/svc_activity_ports.rs` | 513 | 546 | 500 |
| 3 | `tddy-session-lifecycle/src/connection_service/svc_host_builders.rs` | 531 | 544 | 500 |
| 4 | `tddy-session-lifecycle/src/connection_service.rs` | 558 (433 by `restructure lines`) | 567 (442) | 500 |
| 5 | `tddy-accounts/src/service.rs` | 772 | 790 | 500 |
| 6 | `tddy-worktree-service/src/remote_git_service.rs` | 1,004 | 1,111 | 500 |
| 7 | `tddy-session-create-web/src/useCreateSessionPaneState.tsx` | 495 | 503 | 500 |
| 8 | `tddy-rust-typescript-tests/gen/host_pb.ts` (generated) | 1,402 | 1,762 | 500 |
| 9 | `start_cli_harness_session` | 318 | 368 | 60 |
| 10 | `resume_cli_harness_session` | 136 | 166 | 60 |
| 11 | `resume_cli_harness` | 88 | 108 | 60 |
| 12 | `build_session_host` (`tddy-daemon`) | 73 | 82 | 60 |
| 13 | `start_cli_harness_from_request` | 94 | 100 | 60 |
| 14 | `start_sandboxed_cli_harness_session` | 291 | 292 | 60 |
| 15 | `start_session_on_request` | 79 | 80 | 60 |
| 16 | `DaemonSessionHost::new` | 97 | 100 | 60 |

Lines are production lines, counted to the first `#[cfg(test)]`; function lengths are brace-matched, as the records measure them. "Before the stack" is the stack's merge-base with `master`, `3b15ffbc4`; the contract commit re-measured every row there and corrected rows 4, 8, 9, 10, 11 and 13, which the plan commit had read from #921's base.

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
2. **B — `tddy-agent-launch` functions** (rows 10, 11, 13, 14, 15): eleven `extract_method` and one `move_impl_members`. Row 15's function lives in this crate, not in `tddy-session-lifecycle`. Row 9 has no operation: see Decisions. A and B apply in either order, because A's target move re-exports by name and rewrites none of B's anchored functions.
3. **C — `tddy-session-lifecycle`** (rows 2, 3, 4): `move_item` / `move_impl_members` per file. Row 16 has no operation: see Decisions.
4. **D — `tddy-accounts`** (row 5): five child modules of `service`, in two plans. `plan-D.jsonl` moves the helpers, `plan-D2.jsonl` then moves the link `impl` run that calls them; the checker refuses both in one plan.
5. **E — `tddy-worktree-service`** (row 6): three `move_item`: seam A, then seam B in two parts, because seam B as the record designs it is 511 lines.
6. **F — `tddy-daemon`** (row 12): two `extract_method`. The second leaves a `let x = …; x` that `clippy::let_and_return` rejects; the two-line fix after the apply is the one already approved for this engine gap.
7. **G — test support**: the fixture warm-up in the two shared stub writers. Hand edit of test code, no production line.
8. **H — TypeScript** (row 7): hand split.

One plan file per group A–F (two for D), each committed on its own after its apply. Groups G and H are specified in [plans.md](./2026-11-16-remote-shell-sweep/plans.md).

## Callers

| | |
|---|---|
| External importers | listed per plan in [plans.md](./2026-11-16-remote-shell-sweep/plans.md), from `restructure check --deep` |
| Facade | `reexport: "outside"` on every module with a consumer outside its crate, so no other crate's source changes |
| Consequence | no caller outside the moved crates is edited |

## Visibility

Every widening `check --deep` reported is listed per plan in [plans.md](./2026-11-16-remote-shell-sweep/plans.md); each apply's report confirms it. Zero widenings for groups D and E turned out not to be achievable: their child modules read private fields and helpers of the parent.

## Baseline

Recorded in the contract commit, before any change, in [baseline.md](./2026-11-16-remote-shell-sweep/baseline.md), which names every failing test. The known failures on this machine that are not regressions, by cause: the LiveKit testkit's port bind, BSD `cat` rejecting `--` in the sandboxed-codebase suites, an unbuilt `tddy-remote-git-repo` binary, and the `/tmp` path assertion in `claude_cli_from_spec_acceptance`. CI on #921's head (`01a11360e`) is the reference for Linux: all 54 checks pass.

| Gate | Before | After |
|---|---|---|
| `./test -p tddy-agent-launch` | 342 passed, 0 failed | 342 passed, 0 failed |
| `./test -p tddy-session-lifecycle` | 966 passed, 13 failed (12 BSD `cat`, 1 `/tmp` path) | 965 passed, 14 failed (the same 13 by name, plus `the_gh_a_local_gh_session_start_asks_never_sees_the_daemons_github_tokens` under load; that target is 10/10 alone), 1 ignored |
| `./test -p tddy-accounts` | 152 passed, 0 failed | 152 passed, 0 failed |
| `./test -p tddy-worktree-service` | 175 passed, 0 failed, 1 ignored | 175 passed, 0 failed, 1 ignored (after building `tddy-remote-git-repo`, as the baseline did) |
| `./test -p tddy-daemon` | 332 passed, 2 failed (LiveKit testkit port bind) | 333 passed, 1 failed (`restores_a_clone_that_diverged_and_says_so` under load; green alone in 12 s), 1 ignored |
| `tddy-session-create-web` unit tests (`bun test src`) | 104 passed, 0 failed; Cypress not run locally | 104 passed, 0 failed; Cypress not run locally |

State A re-measured after the applies (production lines by `restructure lines`; functions brace-matched; row 4 also whole-file):

| # | After | Verdict |
|---|---|---|
| 1 | `remote_agent_launch.rs` 338 (`startup.rs` 145, `target.rs` 116) | closed |
| 2 | `svc_activity_ports.rs` 412 | closed |
| 3 | `svc_host_builders.rs` 470 | closed |
| 4 | `connection_service.rs` 556 whole · 431 by `restructure lines` (before the stack: 558 · 433) | restored |
| 5 | `service.rs` 421 | closed |
| 6 | `remote_git_service.rs` 418 | closed |
| 7 | `useCreateSessionPaneState.tsx` 498 | closed |
| 8 | `gen/host_pb.ts` 1,762, unchanged | not restored (generated; no seam) |
| 9 | `start_cli_harness_session` 368 (target ≤ 318) | not restored (engine refusals in plans.md; the two `use` lines need the developer's consent) |
| 10 | `resume_cli_harness_session` 135 (target ≤ 136) | restored |
| 11 | `resume_cli_harness` 83 (target ≤ 88) | restored |
| 12 | `build_session_host` 73 (target ≤ 73) | restored |
| 13 | `start_cli_harness_from_request` 84 (target ≤ 94) | restored |
| 14 | `start_sandboxed_cli_harness_session` 287 (target ≤ 291) | restored |
| 15 | `start_session_on_request` 73 (target ≤ 79) | restored |
| 16 | `DaemonSessionHost::new` 100 (target ≤ 97) | not restored (engine refusals in plans.md) |

## Plan

`docs/dev/1-WIP/2026-11-16-remote-shell-sweep/plan-{A,B,C,D,D2,E,F}.jsonl`, written with `restructure anchors`. Each ends `no findings` under `restructure check --deep --budget 500`. [plans.md](./2026-11-16-remote-shell-sweep/plans.md) holds the operations, the projected lengths, the engine's refusals verbatim and the hand-edit specs for groups G and H. No plan has been compiled: the compile gate, clippy and rustfmt run at apply, so every projected length is arithmetic until then.

## Decisions & Trade-offs

- **"Closed or restored" is the bar.** A record the stack added is closed. A record that existed before is brought back to at most its measurement before the stack; where the designed split is on record and nothing above this PR blocks it (rows 5 and 6), it is closed outright.
- **Rows 13–16 grew by one to three lines.** The `#agent-tools` sweep scoped its function tier at five lines. They are attempted here because the bar is "not worse", but rows 12 and 16 carry recorded engine refusals for their remaining seams. A refusal there is recorded in the record with the engine's words, and the row is reported to the developer as not restored.
- **Rows 9 and 16 cannot be restored by the engine.** Row 9 (`start_cli_harness_session`, 368 against 318): every range holding what the stack added carries `Option<Arc<dyn RemoteShell>>`, and the engine refuses a signature naming a type the file does not import. Two hand-written `use` lines in `opencode_cli_spawn.rs` would unlock about 51 lines; that is a hand edit of production code and needs the developer's consent, so it is not in a plan. Row 16 (`DaemonSessionHost::new`, 100 against 97): rust-analyzer offers no extract-into-function assist for the fields of the `Self { … }` literal. Both rows are reported to the developer as not restored.
- **Row 8 has no seam.** `host_pb.ts` is generated from `host.proto`; the only way to shorten it is to split the proto's service, which is a wire change. Whether generated output should be measured against the file budget at all is the developer's call and is put to them when this PR is done. This PR leaves the record as it is.
- **Complexity 5 is a warning.** This is five packages' worth of moves in one PR. It is one concern, the stack's own debt, and each group is a separate plan and commit so a reviewer can take them one at a time. If the plan step shows a group cannot be proven, the group is cut and reported, not forced.
- **Fixture duplication is not moved here.** The stand-in target, stub agents and daemon helpers are repeated across four of the stack's test files. Moving them is a hand edit across test files owned by four PRs; it is recorded for the developer's decision.

## Deferred work — for the stack's TODO gate

Both rows below were **not restored**: the code is left exactly as it is, and no
`docs/dev/todo/` entry is created by this PR. The stack's TODO gate is to ask the
developer about each.

- **Row 9 — `start_cli_harness_session` (368 lines, target ≤ 318).** Not restored
  because the restructure engine refused them: every range holding what the stack
  added carries `Option<Arc<dyn RemoteShell>>`, and the engine refuses a signature
  naming a type the file does not import. Two hand-written `use` lines in
  `opencode_cli_spawn.rs` would unlock about 51 lines. That is a hand edit of
  production code and needs the developer's consent, so it is not in a plan.
  Record: `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md`.
- **Row 16 — `DaemonSessionHost::new` (100 lines, target ≤ 97).** Not restored
  because the restructure engine refused them: rust-analyzer offers no
  extract-into-function assist for the fields of the `Self { … }` literal.
  Record: `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-host-builders-new.md`.

## Final Checklist

Every statement in `packages/*/docs/` that names a moved item's file is listed in [plans.md](./2026-11-16-remote-shell-sweep/plans.md) § Doc statements that name what moves.

## Refactoring Needed

### From /validate-changes

None. The diff contains only this PR's files; no stub, no parent-owned symbol, no
unplanned deletion.

### From /validate-tests

One race, fixed in the test where it lived (see Validation Results): the daemon
clone-restore test asserted the divergence report the instant the restored file
was visible, although the report reaches daemon A asynchronously afterwards.

### From /validate-prod-ready

None. No mock or test-only code in production paths, no added fallback, no
`println!`/`eprintln!` in TUI paths, no new TODO/FIXME (one stale `FIXME(flaky)`
removed with its fix).

### From /analyze-clean-code

None new. Every non-test source file this PR touched is under 500 production
lines (step 3.5 loop over the whole PR range prints nothing). The functions
restored to their before-the-stack lengths stay over the 60-line function budget
as the pre-existing records they are; rows 9 and 16 are under Deferred work.

## Validation Results

### /validate-changes

2026-11-16 (wrap run): stack gate first — base is `master` (PR #924), HEAD
already contains `origin/master` (`7ea536790`), and `origin/master..HEAD` lists
only this PR's fifteen commits: verify-and-return, no rewrite. Diff grouped by
package matches the six packages in Affected Packages plus this PR's own docs.
Boundary checks: every State B row measured in the Baseline table; nothing from
Dependencies implemented (moves only); no dependent behaviour; no unplanned
deletions. Build/clippy in the scoped gates below.

Flaky gate that was red at G: two tests failed under the package run and passed
alone. Method: each run about ten times under the package's normal parallelism,
then repeated with `c431bc926` (group G) reverted in the working tree only
(`git revert --no-commit`, then `git revert --abort`; never committed).

- `tddy-daemon` `restores_a_clone_that_diverged_and_says_so`: reproduced —
  2/10 and 1/10 failures, with and without group G, always at the same line:
  "a reconcile must be reported, never silent". Root cause in the test: the
  mirror restores the clone's file on the peer, and the divergence is reported
  back to daemon A only afterwards (`session_agent_clone.rs`: `report(...)`
  after the restore), so asserting the report the moment the file reads
  restored races it. Fix in the test: wait for the divergence report itself,
  under the same 30 s bound the restore wait already uses; no timeout widened,
  no retry, no ignore. After the fix: 10/10, plus 10/10 under concurrent load.
  Group G did not cause it.
- `tddy-session-lifecycle`
  `the_gh_a_local_gh_session_start_asks_never_sees_the_daemons_github_tokens`:
  not reproduced — 42 runs (binary at default parallelism, at 10 threads, and
  under concurrent daemon-suite load), zero failures. The test binary does not
  include either module group G changed: with `c431bc926` reverted, cargo does
  not even rebuild it. Group G did not cause it, and no race was found in it to
  fix; no speculative change made.

### /validate-tests

Changed test code reviewed against the fluent-tests standard: the two warm-up
builders assert the warm-up's exit status with its stderr; the fixed daemon
test keeps Given/When/Then, one behaviour, and waits on the condition it
asserts instead of on a proxy for it. No always-passing test, no new
`#[ignore]`, no focused test.

### /validate-prod-ready

46 files in the PR range checked: no non-production code in production paths,
no added fallback, no debug output, no TODO/FIXME added. The one `FIXME(flaky)`
in the daemon suite is removed by the race fix above, its hypothesis replaced
by the measured finding.

### /analyze-clean-code

File-length loop (pr-wrap step 3.5, whole PR range, production lines to the
first `#[cfg(test)]`): no file at or over 500. Function lengths are the sixteen
recorded rows: seven closed, seven restored to at most their before-the-stack
length, rows 9 and 16 not restored (Deferred work), row 8 generated and left.

## TODO

- [x] Record initial discovery (`2026-11-16-remote-shell-sweep-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Record the green baseline, by failing-test name
- [x] Write the plans with `restructure anchors`; prove each with `check --deep`
- [x] USER REVIEW — the plan (approved in advance by the developer's standing instruction for this stack's review gates)
- [x] Apply each plan, commit it on its own, `verify --against`
- [x] Hand split of the TypeScript file; fixture warm-up in the shared stub writers
- [x] Final gate: fmt, clippy, the baseline's failing set by name, the comment-line multiset, `verify` against the ref before the first plan
- [x] Re-measure and close or restore every record
- [x] Validate changes (/validate-changes)
- [x] USER REVIEW — scope developed, every gap approved: ready to wrap (standing instruction for this stack's review gates; rows 9 and 16 deferred under Deferred work for the stack's TODO gate)
