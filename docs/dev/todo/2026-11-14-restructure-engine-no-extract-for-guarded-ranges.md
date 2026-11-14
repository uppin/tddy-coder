# 2026-11-14 — `extract_method` has no assist for ranges that start after an early forwarding return

**Category:** Known limitation (engine capability)

**Source:** #901 (`#orchestrators` 12/12, the sweep) — item B2. Standing record:
`packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md`.

## What happened

B2's plan was to `extract_method` the bodies of `query_branch`, `resolve_stack_base`,
`get_pr_status` and `pull_base_into_branch` in
`packages/tddy-daemon-rpc/src/pr_stack/ports.rs` into inherent `serve_*` methods, each range
starting after the body's `forward_if_addressed … return` guard so the range itself would hold no
early `return`, then `move_impl_members` those methods into `pr_stack::stack_reads` and
`pr_stack::pull_base_leg`.

`restructure check --deep` against the warm index refused the extracts: rust-analyzer offers no
*extract into function* assist for these ranges — they hold the `forward_if_addressed … return`
guard and `return Err(..)` guards. (The companion `move_impl_members` refusal, on the crate's
unrelated `#[path]` module, is
`docs/dev/todo/2026-11-14-restructure-engine-path-module-blocks-moves.md`.)

Under green-yolo consent an engine refusal is not done by hand. The developer accepted B2 as
standing debt on 2026-11-14, reason: "restructure engine refused" — recorded on the record above.
`ports.rs` stays at 868 production lines (876 total at the sweep head).

## What would close it

Engine work in `packages/tddy-code-restructuring` (and the rust-analyzer assist it drives):

- Offer *extract into function* for a range that starts after an early forwarding return, when
  the extracted body can be called with `?` — i.e. a range whose own returns are all
  `return Err(..)` must extract to a function the caller invokes with `?`, instead of refusing.
- Pin the shape with a fixture reproducing a `forward_if_addressed … return` guard followed by
  `return Err(..)` guards.

Then re-run B2: re-anchor the four bodies, prove with `restructure check --deep`, apply, and
delete or narrow `packages/tddy-daemon-rpc/docs/code-issues/oversized-file-pr-stack-ports.md`
with its final measurement in the change-history entry.
