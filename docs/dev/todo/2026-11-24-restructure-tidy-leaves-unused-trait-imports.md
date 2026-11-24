# 2026-11-24 — A complete run's tidy leaves an unused trait import in the module a move fills

**Category:** Future enhancement (engine gap)
**Source:** `/pr-wrap` step 3.5 on `feature/session-close-and-remove`: the `close_plan.rs` split
(`docs/dev/1-WIP/2026-11-24-session-close-and-remove.md` § Restructuring)

[`2026-10-04-restructure-move-item-copies-the-whole-use-header.md`](./2026-10-04-restructure-move-item-copies-the-whole-use-header.md)
records that a move copies the source module's whole `use` header, and that "at the end of a
**complete** run the unused-import tidy removes them … so the result is lint-clean". On this run it did
not. Every operation reported `applied`, the compile gate passed, `rustfmt --check` was clean, and
`cargo clippy -p tddy-session-lifecycle --all-targets -- -D warnings` then failed on three copies of one
**trait** import:

```rust
// source: svc_close_session/close_plan.rs (stays, and uses it: `Sha256::digest(…)`)
use sha2::{Digest, Sha256};

// after the run, in three files the moves filled, none of which names a digest:
// svc_close_session/worktree_reads.rs        (move_item, `name` creates the module)
// svc_close_session/conversation_worktrees.rs (move_item, `name` creates the module)
// svc_close_session/close_tabs.rs            (move_impl_members into an existing module)
use sha2::Digest;
```

```text
error: unused import: `sha2::Digest`
 --> packages/tddy-session-lifecycle/src/connection_service/svc_close_session/worktree_reads.rs:1:5
  = note: `-D unused-imports` implied by `-D warnings`
```

The copy itself is expected: a trait import has no name in the moved text, so the header is carried
whole. The gap is the tidy. It kept the other surplus imports out of those files, but left this one. A
guess, not checked: the group `{Digest, Sha256}` was split into one `use` per name, and a trait import
split out of a group is not in the set the tidy asks the compiler about.

**Fixed by hand** (a `use` line per file, deleted). The tree compiled before and after; only the lint
gate saw it.

## What would close it

The tidy prunes every `unused_imports` warning `cargo check` reports in a file the run wrote, trait
imports included, and an acceptance case moves an item out of a module whose header holds
`use sha2::{Digest, Sha256};` into a new module that uses neither, then asserts
`cargo clippy -- -D warnings` is clean.
