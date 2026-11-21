# Changeset: the `#target-parity` sweep — split the files the stack left oversized, and clear a late answer

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Refactor
**Complexity**: 3 — two engine-driven splits in two packages plus one small behaviour fix with its acceptance test

## Affected Packages

- **tddy-agent-launch** — `remote_hook_follower.rs` split; `remote_hook_answer.rs` clears a late answer
- **tddy-worktree-service** — `service.rs` split
- **tddy-session-driver** — passes its spec's answer wait to the sink (signature follow-up of the fix)

## Related Feature Documentation

None for the two splits — behaviour-preserving restructures. The stale-answer fix belongs to
`docs/ft/daemon/1-WIP/PRD-2026-11-15-remote-shell-remote-hooks.md` (a waiting hook on a target).

## Summary

The sweep node of the merged `#target-parity` stack (#969–#975, #988). Three items, chosen by the
developer:

1. **`packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-hook-follower.md`** — 517
   production lines against 500. Intent: one `move_item` of the delivery side
   (`HookReportDelivery`, `ActivityHookSink`, its inherent impl and its `HookSink` impl) into a new
   module `remote_hook_delivery`, re-exported from `remote_hook_follower` for the callers outside the
   crate (`tddy-session-lifecycle`, `tddy-session-driver`, the acceptance tests). Closes the record.
2. **`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`** — 831 production
   lines against 500. Intent: seams B (stats/size: `stream_worktree_stats`,
   `calculate_worktree_size`) and C (file reading: `list_worktree_directory`, `read_worktree_file`,
   `stream_read_worktree_file`). Each trait method body is rebound off `self`
   (`read_fields_through`, self mode) and extracted as a free function (`extract_method`), the trait
   method keeping a delegating call; the free functions then move into child modules of `service`
   (`move_item`), which still see the service's private fields and helpers. Closes the record if the
   result is under 500, otherwise narrows it with the new measurement.
3. **A late answer is left on the target** (behaviour change, own commit, test first). A waiting hook
   on a target polls `.tddy/answers/<nonce>` for `answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS`. A
   Coder write takes 6–9 s, so an answer the daemon writes after the hook gave up is never read and
   stays until the session ends. Fix: after writing an answer, the daemon removes it at the end of the
   hook's window — at once when the window has already passed. The sink is told the session's
   `answer_wait_secs` (`ActivityHookSink::answering_through` takes it), so the window is the same
   number the hook was rendered with.
   - Test: `remote_hooks_acceptance.rs` — `an_answer_written_after_the_hook_gave_up_is_not_left_on_the_target`,
     an `AnOrchestrator` that answers after a short hook wait; asserts no answer file is left once the
     window has passed.

## Baseline

Recorded before the first move, re-run after the last (by test name):

| Suite | Before | After |
|---|---|---|
| `tddy-agent-launch` `--lib` + `remote_hooks_acceptance`, `remote_hook_follower_acceptance`, `remote_hook_log_tail_ends`, `remote_agent_launch_acceptance`, `target_agent_tab_acceptance`, `coder_agent_launch_acceptance`, `spec_launch_acceptance` | lib 359 ok; 1 pre-existing timing failure `remote_agent_launch_acceptance::an_agent_on_a_target_that_never_answers_fails_the_start_after_its_bound` (`AgentNeverStarted` vs `AgentNeverAnswered` under load); the new test red as intended | `remote_hooks_acceptance` 12/12 after the fix |
| `tddy-worktree-service --lib` + every acceptance suite but `remote_git_livekit_acceptance` | all ok except 1 pre-existing failure `worktree_size_calculator_acceptance::a_cached_size_is_served_after_reload_without_recomputing` | not re-run: no move applied |

## Scope

- [x] **Plan**: changeset, baseline
- [x] **Stale answer**: failing acceptance test, then the fix (commit `805085c8f`)
- [ ] 🔴 **Split remote_hook_follower.rs** — refused by the engine (below)
- [ ] 🔴 **Split service.rs** (seams B, C) — refused by the engine (below)
- [ ] **Baseline re-run**, clippy `-D warnings`, fmt
- [ ] **Docs**: records closed or narrowed, changesets and changelogs wrapped

## Engine refusals (2026-11-21) — waiting for the developer

Both splits stopped at `restructure check --deep`; nothing was written. Plans in `tmp/restructure/`.

1. `follower.jsonl` (`move_item` of the delivery side into `remote_hook_delivery`, `reexport: outside`,
   and again with `named`):
   `plan is malformed: \`mod tests;\` leads to neither \`tests.rs\` nor \`tests/mod.rs\` in
   packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs`. The engine's module walk ignores the
   `#[path = "restore_tabs_tests.rs"]` on that declaration (`opencode_cli_spawn/restore_tabs.rs:26`) —
   an engine defect unrelated to the moved items.
2. `service-c1.jsonl` (seam C, `list_worktree_directory`: `read_fields_through` self mode, then
   `extract_method` over the same item-relative range, one group): `group \`c1\` is refused whole …
   op 1: this seam cannot be cut here: rust-analyzer offers no "extract into function" assist for the
   given range (it offered: Extract into variable)`. A second item anchor shifted past the inserted
   `let` cannot be written: anchors resolve against the tree at run open.

Also measured: seams B and C together cannot bring `service.rs` under 500 even when they apply — the
five trait-method signatures, doc comments and delegating calls stay, so ~590 lines remain, not the
record's ~440. Closing it needs a further seam (the inherent resolve helpers, `move_impl_members`).
