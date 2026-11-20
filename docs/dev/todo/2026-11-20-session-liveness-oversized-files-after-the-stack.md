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
