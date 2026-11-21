# 2026-11-20 — `#session-liveness` 1/4 left five stack-shared files over the 500-production-line budget

**Category:** Debt (deferred to after the stack lands)
**Source:** `/pr-wrap` step 3.5 on #958 (`#session-liveness` 1/4)

Each file below is also edited by `#session-liveness` 2/4, 3/4 or 4/4 (#959, #960, #961), so splitting
it inside node 1 would turn every successor into a rename conflict. The developer's rule for this stack:
a file the successors also modify is recorded, not split. Production lines are counted to the first
`#[cfg(test)]`.

| File | Before → after | Record |
|---|---|---|
| `packages/tddy-session-lifecycle/src/test_util.rs` | 484 → 578 (crossed 500) | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-test-util.md` |
| `packages/tddy-daemon-sandbox/src/sandbox_session.rs` | 885 → 924 | `…/oversized-file-sandbox-session.md` (row added) |
| `packages/tddy-daemon-rpc/src/pr_stack/ports.rs` | 878 → 880 | `…/oversized-file-pr-stack-ports.md` (row added) |
| `packages/tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs` | 521 → 527 | `…/oversized-file-telegram-session-control-chaining-and-listing.md` (row added) |
| `packages/tddy-telegram-control/src/telegram_session_control/pickers.rs` | 627 → 628 | `…/oversized-file-telegram-session-control-pickers.md` (row added) |

**What closes it:** once #961 (the last node) has merged, split the file this stack pushed over 500
along the seam its record names (`test_util.rs`: the held-agent helpers into
`test_util/held_agents.rs`), move-only through `/code-restructuring`, so the engine proves the same
green before and after. The four files that were already over budget keep their own records.

**Closed inside the stack:** `svc_signal_delete_session.rs` (446 → 511 in #958, 538 in #960) is no
longer on this list. #961 does not edit it, so #960's wrap split it — the signal half moved to
`svc_signal_session.rs` (538 → 384 and 159 production lines), `signal_held_session` kept its old
path through a `pub use`, and its record was deleted on that measurement.

## Also left for after the stack: the new-tab menu's repeated groups (#965, `#session-liveness` 6/8)

`packages/tddy-session-web/src/NewTerminalMenu.tsx` (464 production lines, under the budget) renders the
"In sandbox" / "On host" group pair (#964) and, since #965, an "On target" / "On this host" pair that repeats
its markup with other labels, test ids and wrapper kinds. `sandbox-agent-tab` (7/8) and `target-agent-tab`
(8/8) each edit one of the two blocks again (they list the agents in them), so folding both into one
`WrappedKindGroups` component inside node 6 would turn each successor into a conflict in the very lines it
changes. **What closes it:** once #967 has merged, extract one component taking the group label, the wrapper
kind, the test-id prefixes and the two group names; the menu's Cypress specs
(`NewTerminalMenuSandboxAcceptance`, `NewTerminalMenuTargetAcceptance`) stay as they are.

## Left for after the stack: files #966 (`#session-liveness` 7/8) grew that #967 also edits

No file #966 touched is at or over 500 production lines, so nothing was split or recorded as crossing. Three
files #967 (`target-agent-tab`) also modifies came close, and were left whole for that reason:
`packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs/roster.rs` (446 → 494, 6 lines under the
budget), `…/restore_tabs/plan.rs` (263 → 305) and `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports/jailed_tabs.rs`
(90 → 176; #967 edits its sibling `target_tabs.rs`, not this one, and it is far under). `roster.rs` will cross 500
the moment #967 adds its restore arm. **What closes it:** after #967 merges, move the per-kind restores
(`restore_jailed_shell`, `restore_jailed_agent`, `restore_target_shell`, `relaunch_tmux_tab`) into
`restore_tabs/restore_one.rs`, move-only through `/code-restructuring`. `NewTerminalMenu.tsx` (468 production
lines) gained its agent items in the "In sandbox" block (still under budget); the group-pair dedupe above now
covers both blocks.
