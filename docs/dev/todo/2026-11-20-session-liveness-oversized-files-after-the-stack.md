# 2026-11-20 — `#session-liveness` 1/4 left five stack-shared files over the 500-production-line budget

**Category:** Debt (deferred to after the stack lands)
**Source:** `/pr-wrap` step 3.5 on #958 (`#session-liveness` 1/4)

Each file below is also edited by `#session-liveness` 2/4, 3/4 or 4/4 (#959, #960, #961), so splitting
it inside node 1 would turn every successor into a rename conflict. The developer's rule for this stack:
a file the successors also modify is recorded, not split. Production lines are counted to the first
`#[cfg(test)]`.

| File | Before → after | Record |
|---|---|---|
| `packages/tddy-session-lifecycle/src/test_util.rs` | 484 → 578 (crossed 500); 578 → 556 in #990, which moved its tool hosts out (still over; #991 and #992 also edit it) | `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-test-util.md` |
| `packages/tddy-daemon-sandbox/src/sandbox_session.rs` | 885 → 924 | closed in #991, see below (record deleted) |
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

## Closed in #990 (`#tab-hardening` 2/4): the new-tab menu's repeated groups

The "In sandbox" / "On host" and "On target" / "On this host" group pairs of
`packages/tddy-session-web/src/NewTerminalMenu.tsx` are one `WrappedKindGroups` component now (items are
`MenuKindItem`; the test ids are unchanged and the two menu specs are untouched). `NewTerminalMenu.tsx` went
473 → 424 production lines.

## Closed in #991 (`#tab-hardening` 3/4): `sandbox_session.rs`

`packages/tddy-daemon-sandbox/src/sandbox_session.rs` was 938 production lines at #990 and 1,037 with #991's death
watch wiring. #991 (no other `#tab-hardening` node edits it) moved three clusters out unchanged, re-exported from
`sandbox_session` so no path moved: `jail_host_bridge.rs` (360), `runner_paths.rs` (146) and `runner_spawn.rs` (144).
The file is **416** production lines; its `oversized-file` record was deleted on that measurement.
`dial_and_bridge`'s 12 parameters remain, in `heavy-params-jail-host-bridge-dial-and-bridge.md`.

## Closed in #967 (`#session-liveness` 8/8): `restore_tabs/roster.rs`

`packages/tddy-agent-launch/src/opencode_cli_spawn/restore_tabs/roster.rs` was 494 production lines at #966 and
would have crossed 500 with #967's restore arm (540). #967 is the top node, so it split the file itself: the
per-kind restores of a jailed or target tab (`restore_jailed_shell`, `restore_jailed_agent`, `restore_target_shell`,
`restore_target_agent`) moved, unchanged, into the child module `restore_tabs/roster/wrapped_tabs.rs` (a child of
`roster.rs`, so it reads the private helpers without widening them); `roster.rs` is now 408 production lines
`restore_tabs/plan.rs` (263 → 339) and `jailed_tabs.rs` stay far under the budget.
