# 2026-11-21 — A late answer is cleared from the target

**Type:** Bug Fix · **Stack:** `#target-parity` sweep

`remote_hook_answer`: after writing a waiting hook's answer, the daemon removes it once the hook's window
(`answer_wait_secs + AWAIT_CLIENT_MARGIN_SECS` plus 3 s) has passed — at once when it already has — so an
answer that landed after the hook gave up no longer stays in `.tddy/answers/` until the session ends.
`ActivityHookSink::answering_through` takes the session's `answer_wait_secs`. The planned split of
`remote_hook_follower.rs` was refused by the restructure engine and deferred (526 production lines).
Full record: [`docs/dev/changesets/2026-11-21-target-parity-sweep.md`](../../../../docs/dev/changesets/2026-11-21-target-parity-sweep.md).
