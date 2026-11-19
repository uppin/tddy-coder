# Changeset: The `#coder-start` final sweep

**Date**: 2026-11-19
**Status**: ✅ Implemented (wrap pending)
**Type**: Refactor
**Complexity**: 2. One function cut by four engine `extract_method`s, two test-fixture consolidations, four backlog entries updated, one design question written up. No behaviour change.
**Stack**: `#coder-start` 10/10 (sweep). Branch `feature/coder-start/coder-start-sweep`, base `feature/coder-start/target-test-kit` (#949).

## Initial Discovery

[2026-11-19-coder-start-final-sweep-initial-discovery.md](./2026-11-19-coder-start-final-sweep-initial-discovery.md)

## Dependencies

Inherits every other node of the stack (#944–#949); it implements nothing of theirs.

## Affected Packages

- **tddy-agent-launch**: `start_cli_harness_session` (`src/opencode_cli_spawn.rs`) gives four steps to private helpers in the same file. `tests/acme_target_support` is new. No dev-doc change: no doc names the function's body.
- **tddy-session-lifecycle**: `tests/derived_host_support` is new. No dev-doc change.

No PRD: this is a behaviour-preserving sweep. The pipeline's sweep node is planned without one.

## Backlog and outcome

| Item | Outcome |
|---|---|
| [`complexity-opencode-cli-spawn-start-cli-harness-session.md`](../../../packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md) | **397 → 371**. The stack's own growth (383 at `8b004d090` → 397) is undone, ending 12 below the base. The record is narrowed and stays open: ≤ 60 needs parameter structs over the start's inputs. |
| [`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`](../../todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md) | Narrowed. The identical copies are consolidated into two support modules. What remains has drifted. |
| [`2026-11-16-coder-hosts-never-run-against-a-real-deployment.md`](../../todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md) | Narrowed. `stopped` was read from the real deployment. The hook follower is still unverified. |
| design question from #948's `[F]` stage | New: [`2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md`](../../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md). Production behaviour is unchanged. |
| `TODO`/`FIXME` markers | None was added by the stack, so nothing to solve. |
| `complexity-remote-agent-launch-launch-on-target.md` (6 parameters > 5, from #944) | Left open. Bundling the parameters changes a signature that `tddy-session-driver` calls, so it is not a restructure. |
| [`2026-11-18-idle-session-reported-as-waiting-on-a-permission.md`](../../todo/2026-11-18-idle-session-reported-as-waiting-on-a-permission.md) | Left open: two read oddities that were never reproduced. |

## Plan

`2026-11-19-coder-start-final-sweep-plans/plan-1-start-cli-harness-session.jsonl`: four `extract_method`s, bottom-up, with anchors emitted by `restructure anchors` against the warm index. `check --deep` and the dry run were clean, and the apply passed its compile gate.

| Helper | Lines | Parameters |
|---|---|---|
| `report_the_started_session` | 22 | 5 |
| `place_slot_files_and_pointer` | 21 | 5 |
| `log_hooks_on_the_target` | 12 | 2 |
| `node_launcher_for` | 9 | 2 |

## Decisions & Trade-offs

- **The target-shell resolution stays inline.** `check --deep` refused it: the extracted signature names `CoderCredentialSource`, which `opencode_cli_spawn.rs` does not import. Unlocking it needs a `use` line written by hand before the move, and the restructure rules allow hand edits only after a move. Decided by the orchestrator, under the user's finish-without-stopping instruction (recommended option: no pre-move hand edit).
- **Only ranges with ≤ 5 inputs were cut**, so the sweep adds no parameter-count finding.
- **`verify --against`**: no tokens lost. The six "missing" statements are the same lines with `&`/`&mut` rebound to the new parameters.
- **Test fixtures were consolidated by hand.** Each `tests/*.rs` file is its own crate, so the engine has no move between them. Bodies are byte-identical apart from `pub`, and the one difference (the project id) became a parameter. Assertions are unchanged.
- **The design question is not decided here** ([the new entry](../../todo/2026-11-19-a-target-start-requires-the-agent-on-the-daemon-host.md)). It is a product decision about what a target start requires of the daemon's host.

## Verification

- Scoped: `./dev cargo clippy -p tddy-agent-launch -p tddy-session-lifecycle --all-targets -- -D warnings` was clean, `cargo fmt --check` was clean on both packages, and the comment-line count of `opencode_cli_spawn.rs` is 51 before and after.
- ⚠️ `./test -p tddy-agent-launch -p tddy-session-lifecycle` was **not run locally**. Free disk on the machine fell to the 15 GiB floor (12 GiB, other worktrees building), so the test run is left to CI. The baseline is #949's green rollup on `c965b89a7`.

## Final Checklist

- [ ] `packages/tddy-agent-launch/docs/changesets/2026-11-19-coder-start-final-sweep.md`: release note
- [ ] `packages/tddy-session-lifecycle/docs/changesets/2026-11-19-coder-start-final-sweep.md`: release note
- [ ] `docs/dev/changesets/2026-11-19-coder-start-final-sweep.md`: cross-package entry
