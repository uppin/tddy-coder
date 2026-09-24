# 2026-09-24 — `a_cached_size_is_served_after_reload_without_recomputing` races the write it reads

**Category:** Defect (test)
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), the DRY-target
baseline of changeset
[`2026-09-23-carve-lifecycle-destructure`](../1-WIP/2026-09-23-carve-lifecycle-destructure.md)

`packages/tddy-worktree-service/tests/worktree_size_calculator_acceptance.rs` is flaky on master's
code. At `3a96ca22`, with nothing in `tddy-worktree-service` or `tddy-core` changed since
`origin/master`, it failed 2 of 4 runs back to back. On #524's tree, whose `tddy-worktree-service`
differs only by one added method (`MpscResultStream::into_receiver`), it failed 8 of 8 in a row
and passed in the baseline run before that. The failure is always the same assertion:

```text
thread 'a_cached_size_is_served_after_reload_without_recomputing' panicked at
packages/tddy-worktree-service/tests/worktree_size_calculator_acceptance.rs:273:5:
assertion `left == right` failed
  left: None
 right: Cached
```

## Why

`WorktreeSizeCalculator::enqueue` (`src/worktrees.rs`) spawns a task that marks the worktree
`Cached` in memory, releases the lock, and only **then** persists the size. The test waits for the
in-memory state and reloads straight away:

```rust
// the spawned task
guard.states.insert(key, WorktreeSizeState { status: Cached, … });   // visible to `state()` now
…                                                                    // lock released
persist_worktree_size(&root, &persist_lock, &project_id, &path, bytes, at);   // file written after

// the test
await_cached_size(&calc, &path, 4096).await;       // returns as soon as the insert is visible
let reloaded = WorktreeSizeCalculator::with_sizer(tmp.path().to_path_buf(), 2, …);
let state = reloaded.state(PROJECT, &path);        // reads the file, which may not exist yet
assert_eq!(state.status, WorktreeSizeStatus::Cached);
```

Whichever thread runs first decides the result.

## What would close it

Either the test waits for the persisted file, or `enqueue` persists before it publishes `Cached`, so
that "Cached" means "a reload will see it". The second is the behaviour the test's name claims. It is
a product change in `tddy-worktree-service`, which is why #524 (a behaviour-preserving restructure)
records it rather than fixing it.
