# 2026-09-25 — the warm index daemon serves the **main checkout** to a client standing in a nested worktree

**Category:** Future enhancement (engine defect; a warm `apply` would have written into the wrong tree)
**Source:** `#carve` 15/15, [#526](https://github.com/uppin/tddy-coder/pull/526), running
`tddy-tools restructure check --deep` from `/Users/mantasi/Code/tddy-coder/.worktrees/feature-carve-lifecycle-split-1`
with `TDDY_INDEX_SOCKET` set by that worktree's own `./run-index-daemon`

## What happened

The client sends `std::env::current_dir()` as `workspace_root`, which is correct. The daemon then
passes that root through `tddy_lsp::registry::workspace_root_for`, which walks **outward** and keeps
the **outermost** ancestor holding a `Cargo.toml`:

```rust
// packages/tddy-lsp/src/registry.rs
for ancestor in target_dir.ancestors() {
    if ancestor.join("Cargo.toml").is_file() {
        root = Some(ancestor.to_path_buf());   // the last (outermost) hit wins
    }
}
```

A worktree at `<main>/.worktrees/<name>` has the main checkout as an ancestor, and that ancestor has
a `Cargo.toml` too. So every request was answered for the main checkout. At the time, that checkout
was on a different branch (`fix/desktop-hook-shell-quoting`):

```text
[tddy_index_daemon::activity] check arrived for `/Users/mantasi/Code/tddy-coder`, which has no index yet
[tddy_index_daemon::activity] check for `/Users/mantasi/Code/tddy-coder`: refused as FailedPrecondition (+0ms):
    no plan at `/Users/mantasi/Code/tddy-coder/docs/dev/1-WIP/2026-09-23-carve-lifecycle-wiring-plans/02b-….jsonl`
```

A relative plan path exposed it, because the plan exists only in the worktree. A plan named by an
absolute path gave **no sign**. Three `check --deep` runs returned findings computed against the
other branch's tree, and they read exactly like findings about this one. `apply` takes the same
root, so a warm apply would have edited, staged and journalled in the main checkout. Nothing was
written: every run was a `check`, and the main checkout's `git status` was clean afterwards.

The cold path (`TDDY_INDEX_SOCKET` unset) is unaffected. `restructure_cli` uses the process
directory as the root and hands it to rust-analyzer unchanged.

## Workaround used

`env -u TDDY_INDEX_SOCKET` for every run from a nested worktree. That is cold, about 6–10 minutes
per run.

## What would fix it

`workspace_root_of` should keep the directory the client named when it is itself a cargo workspace
root, meaning a `Cargo.toml` with `[workspace]`. It should also stop at a `.git` boundary, whether a
directory or a worktree's `.git` file. The outermost-`Cargo.toml` rule exists to lift a member
crate's directory up to its workspace, and it should never cross into an enclosing repository.
`tddy-lsp-executor` calls the same function twice, so it has the same exposure.
