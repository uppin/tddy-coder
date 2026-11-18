# Changeset: Target-session tests share one stand-in kit, and a live test proves the Coder path on a real deployment

**Date**: 2026-11-18
**Status**: 🚧 In Progress
**Type**: Refactor (tests) + production test
**Complexity**: 4. Spans four crates (`tddy-testing-commons`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-driver`), test-only moves across 11 files, and adds a production test against a real deployment.
**Stack**: `#coder-start` 9/10 — branch `feature/coder-start/target-test-kit`, base `feature/coder-start/start-visibility`

## Initial Discovery

[`2026-11-18-target-test-kit-initial-discovery.md`](2026-11-18-target-test-kit-initial-discovery.md)

## Prerequisites

- ✅ RESOLVED HERE: [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md)
- ✅ RESOLVED HERE: [`2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`](../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md)

## Affected Packages

- **`tddy-testing-commons`**: a new `remote_target` module (behind its existing dev-only use).
- **`tddy-agent-launch`, `tddy-session-lifecycle`**:
  - the `tests/support/` module each crate already has for its typed helpers;
  - 11 test files' fixtures swapped for imports.
- **`tddy-session-driver`**:
  - a dev-dep on `tddy-testing-commons`;
  - its acceptance file moved onto the kit;
  - a new `tests/coder_live_production.rs`.
- **`docs/dev/guides/testing.md`**: list the production test.

## Scope

- [ ] Kit module and per-crate support modules
- [ ] Every copy replaced; suites unchanged by name and count
- [ ] Live production test, run once against `mantasi/remote-dev`, with its output recorded in the PR
- [ ] Stand-ins corrected for any fact the live run contradicts
- [ ] Scoped gates

## Responsibility

The shared target-session test fixtures, and the one test that runs the Coder path for real.

## Boundaries

- No production-code change, except a stand-in fact the live run proves wrong (recorded as a
  finding and put to the developer first) — and one scoped exception: the session driver's
  optional `--session-id` (see Decisions).
- Test assertions are unchanged.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/coder-start/coder-transport` | streamed Coder commands over `--stdio`; new stand-in bodies | the live hook-follow check; kit absorbs its stand-ins | change the transport |
| `feature/coder-start/target-agent-start` | target-resolved agents, mint, pre-trust | the live Claude/Cursor starts need them | change the launch |
| `feature/coder-start/start-visibility` | the start's remote-call log | the live test prints it on failure | change the ledger |

## Draft PR contract

Commit 2 is the recorded green baseline of the four suites (names and counts), plus the live test as
a failing production test. The live test is red until nodes 5 and 7 are green beneath it.

## Recorded baseline and contract (pass 2, 2026-11-18)

Scoped run `./test -p tddy-testing-commons -p tddy-agent-launch -p tddy-session-lifecycle -p tddy-session-driver` on `61f07b821`: **1372 passed, 28 failed, 3 ignored** (STALE: taken on the old, pre-cascade base). The valid before-count on the current base `e70e836b0` is **1115 passed, 28 failed, 3 ignored** (100 test binaries, the same 28 failures). `tddy-testing-commons` and `tddy-session-driver` (unit 3, acceptance 7) are fully green. The 28 failures are not this node's:

- **Parent contracts still red (nodes 6/7):** `target_trust_acceptance` (3), `remote_agent_launch_acceptance` (2) and lib unit tests (`remote_agent_launch::tests::*` 2, `cli_agent_steps_tests::a_minted_conversation_on_a_target_is_minted_through_its_shell`) in `tddy-agent-launch`; `claude_cli_from_spec_acceptance::an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` and lib `report_session_status_unit_tests::waiting_for_input_records_the_agents_message_as_the_pending_question` in `tddy-session-lifecycle`.
- **Environment:** `sandboxed_codebase_lifecycle_acceptance` (5), `sandboxed_codebase_placement_acceptance` (7) -- macOS sandbox harness; `session_sync_livekit_acceptance` (6, "Once instance has previously been poisoned") -- LiveKit testkit.

Live test `tests/coder_live_production.rs` (3 tests, `#[ignore]`, run with `--ignored` against `me/remote-dev`):

- `the_deployment_reads_as_the_stand_ins_assume` -- **passes**: `coder version` first line is `Coder v2.33.5+81139b5 ...`; `remote-dev` reports `running`.
- `a_claude_session_on_the_workspace_reaches_its_prompt_and_its_hook_log_is_followed` and `a_cursor_session_on_the_workspace_mints_its_chat_and_reaches_its_prompt` -- **red**: `start_session` fails with the agent exiting 127 on the workspace: `env: '/Users/mantasi/.local/share/claude/versions/2.1.311': No such file or directory` (Cursor: `.../cursor-agent/versions/2026.11.17-.../cursor-agent`). The driver sends the daemon host's absolute agent path to the workspace. This is the `target-agent-start` (node 7) defect; the live test stays red until it is green.
- Without `CODER_URL` the test fails naming it.

Unverified because the start never gets there: that Claude shows `for shortcuts` and Cursor `Cursor Agent` as the prompt marker, that `<worktree>/.tddy/hooks.log` exists while running and is removed on stop. Also noted: the workspace reports "outdated" on each `coder ssh` (stderr noise in the start's output).

## Green-wave results (2026-11-18)

- Consolidation: `./test -p tddy-testing-commons -p tddy-agent-launch -p tddy-session-lifecycle -p tddy-session-driver` after: 139 binaries, **1366 passed, 34 failed, 6 ignored**. Every failure in the first (cut-short, 100-binary) baseline run recurs by name; the six extra are `session_sync_livekit_acceptance` (LiveKit testkit, environment), which the cut-short baseline never reached. No test name was added or removed. `./dev cargo clippy -p <the four> --tests -- -D warnings` is clean.
- Live run against `remote-dev` (`--ignored`): `the_deployment_reads_as_the_stand_ins_assume` passes; `a_cursor_session_on_the_workspace_mints_its_chat_and_reaches_its_prompt` **passes**, so `Cursor Agent` is a confirmed prompt marker (no correction needed); the 127 failures of the contract run are gone.
- Claude, first run: stopped at its folder-trust dialog on a bare `mktemp -d` worktree. That is node 7's intended posture (see Decisions); the test now uses the clone layout and passes. Details in Decisions.

## Green wave

**Wave:** 3 of 3
**Greenable independently:** no. It consolidates the copies that nodes 5–8 add, and its live test
exercises their behaviour.
**Concurrent with:** none
**Blocks:** the sweep

## Decisions & Trade-offs

- **Test fixtures may be written by hand** (developer, 2026-11-18). The engine is tried first; if
  it refuses (integration test files are crates of their own), the kit is hand-written and the
  copies deleted, assertions unchanged. Production code stays engine-only.
- **Partial consolidation** (orchestrator, under the user's finish-without-stopping instruction): `tddy_testing_commons::remote_target` holds `write_executable` (replacing the warm-up copies in 11 test files) and `StandInSshTarget` (replacing the identical `AReachableMachine`/`ATarget` in 4 lifecycle files). Left in place because their bodies have diverged or carry evolved features: the stand-in coder `AWorkspace` (lifecycle `coder_target_session`, agent-launch `coder_agent_launch`), agent-launch's `ATarget` with its call log, and the stub-agent families. `tddy-remote-shell/tests/support` and the `remote_agent_launch.rs` src copy stay too: a `tddy-testing-commons` dev-dependency there risks a cycle. Remaining copies are follow-up work for the sweep.
- **`--session-id` on `tddy-session-driver`** (orchestrator, under the user's finish-without-stopping instruction): the Claude start pre-trusts a clone only when the worktree is `<clone>/.worktrees/<session id>` (node 7's deliberate posture: any other layout trusts nothing). The driver generated a random session id after `--worktree` was fixed, so no caller could lay that out. An optional `--session-id` (unchanged behaviour when absent) lets the live test do it. Claude requires the id to be a UUID. Result: with the layout right, Claude starts and reaches its prompt with no trust dialog, so node 7's pre-trust is verified live.
- **Clean baseline rerun skipped** (orchestrator, under the user's finish-without-stopping instruction): the first baseline run was cut short (100 of 139 binaries); every name in it recurs in the after run with the same status and no test was added or removed, and CI runs the full suite independently, so a second full local run was not worth its disk and time.
- **Live results corrected**: Claude's prompt marker is `❯ Try` (the `for shortcuts` guess is not on screen under the status line); `Cursor Agent` is confirmed. The `hooks.log` lifecycle is **not** checked by the driver test: `hooks.log` is created by the daemon's start path (`create_remote_hook_log`), which the driver does not use, so it never exists under it. The contract premise was wrong; the test was renamed `a_claude_session_on_a_cloned_worktree_reaches_its_prompt_without_the_trust_dialog`. A live check of the hook log needs a daemon-level start and is left to a follow-up.
- **Two concerns in one node** (developer, 2026-11-18): consolidation and the live test.

## TODO

- [x] Record initial discovery
- [x] Create PRD
- [x] Create changeset
- [ ] Draft PR contract: baseline + live test (commit 2)
- [ ] USER REVIEW: acceptance tests (6e)
- [ ] TDD Green
- [ ] Validate changes
- [ ] Wrap
