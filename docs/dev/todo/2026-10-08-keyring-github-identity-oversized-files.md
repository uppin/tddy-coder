# 2026-10-08 — #keyring 9/9 left files over the 500-production-line budget

**Category:** Technical debt
**Source:** `#keyring` 9/9 (PR #516) `/pr-wrap` file-length gate, deferred with the developer's consent

Files this PR grew while already over budget (production lines before the first `#[cfg(test)]`, before → after). Decomposing them inside the PR would have turned each parent and dependent PR that touches the same file into a conflict, so the split is a follow-up branch after the stack lands.

| File | Before → after |
|---|---|
| `packages/tddy-toolcall/src/toolcall/listener.rs` | 676 → 766 (the token handler dispatch is the growth; the natural seam) |
| `packages/tddy-daemon-rpc/src/pr_stack/ports.rs` | 775 → 808 |
| `packages/tddy-presenter/src/presenter/workflow_runner.rs` | 1015 → 1036 |
| `packages/tddy-supervisor/src/supervisor.rs` | 948 → 965 |
| `packages/tddy-daemon/src/runtime.rs` | 1776 → 1781 |
| `packages/tddy-session-lifecycle/src/cursor_cli_spawn.rs` | 532 → 548 |
| `packages/tddy-telegram-control/src/telegram_session_control/{session_start,pickers}.rs` | +2 and +1 lines |

Use `/code-restructuring` (engine-driven moves, green baseline first). Record each as a `packages/<pkg>/docs/code-issues/oversized-file-<slug>.md` when picked up.
