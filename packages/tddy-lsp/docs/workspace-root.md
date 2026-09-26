# Workspace roots (tddy-lsp)

`LspRegistry` keys its servers by `(workspace root, language)`, so which root a path resolves to
decides which running server — and so which tree — answers for it. The product contract is
[Reusable LSP](../../../docs/ft/coder/reusable-lsp.md); this page is how the root is found.

## `registry::workspace_root_for`

It follows `cargo locate-project --workspace`, bounded by the repository. Walking outward from the
target directory:

1. the nearest `Cargo.toml` that declares `[workspace]`, or a `[workspace.*]` table
   (`declares_workspace`, with comments ignored), is the root;
2. otherwise the nearest `Cargo.toml` of any kind is: a package in no workspace is its own root;
3. otherwise the target directory itself is.

**The walk stops at the first directory holding `.git`** — a directory for a checkout, a file for a
linked worktree. So a worktree nested inside another checkout, such as `<main>/.worktrees/<name>`, is
rooted at its own workspace and served its own tree. Without that bound the enclosing checkout's
workspace manifest would win, and every request from the worktree would be answered, checked and
applied against another branch's files.

**An unreadable manifest on the walk is an error** (`LspError::Io`, naming the file), not a manifest
skipped. `tddy-index-daemon` reports it as `FailedPrecondition`; `tddy-lsp-executor`, which resolves
its roots with the same function, as its string error.

## Tests

`tests/workspace_root_test.rs` builds each shape on disk: a nested worktree rooted at its own
workspace, a member of that worktree lifted to it, a member crate lifted to the workspace holding
it, no crossing of a `.git` boundary into an enclosing workspace, and a directory with no manifest
rooted at itself.

`tests/bin/fake_lsp.rs` is the deterministic server other crates test against. Besides the document
versions and sync notifications it replays, `tddy/watchedFileChanges` returns the
`workspace/didChangeWatchedFiles` changes a client sent, in arrival order, which is how
`tddy-index-daemon` asserts that it told a warm server what changed on disk.

## Related

- [Reusable LSP](../../../docs/ft/coder/reusable-lsp.md) — the registry, targets and idle reaping
- [`tddy-index-daemon`'s warm state](../../tddy-index-daemon/docs/code-index-service.md#warm-state-per-workspace-root)
- [changesets/](./changesets/)
