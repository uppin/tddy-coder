# 2026-11-24 — The tidy narrows a test-only import to `#[cfg(test)] use` at the top, which blinds the file-length gate

**Category:** Future enhancement (engine gap)
**Source:** `/pr-wrap` step 3.5 on `feature/session-close-and-remove`: the `close_plan.rs` and
`worktrees.rs` splits (`docs/dev/changesets/2026-11-24-session-close-and-remove.md` § Restructuring)

When a move takes the last production use of an imported name, and only the file's own
`#[cfg(test)] mod tests { use super::*; … }` still needs it, the run's tidy keeps the import but gates
it **in place, in the header**:

```rust
// worktrees.rs after the moves (tddy-worktree-service)
//! Git worktree listing, stats cache, path policy, and removal helpers (Worktrees manager PRD).

#[cfg(test)]
use std::fs;                                   // only the tests' fixtures write files now
use std::path::{Component, Path, PathBuf};
…
pub use crate::worktree_stats_cache::WorktreeStatsCache;

#[cfg(test)]
use crate::worktree_sizes::directory_size_bytes_best_effort;   // only a test calls it
pub use crate::worktree_sizes::{ … };
```

The same happened to `close_plan.rs` (`#[cfg(test)] use tddy_service::proto::session::CloseSessionDirtyFile;`
at line 13).

It compiles and it is lint-clean. But this repo measures a Rust file's **production lines up to the
first `#[cfg(test)]`**: the `/pr-wrap` step 3.5 gate, every `oversized-file` record, and
`restructure check --budget` all count that way. A gated import in the header stops the count there:

```text
$ awk '/^[[:space:]]*#\[cfg\(.*test[),]/ {exit} {n++} END {print n+0}' worktrees.rs
2          # the file has 409 production lines
$ … close_plan.rs
12         # 409
```

So a split that does not reach the budget would still pass the gate, and a later PR that grows the
file would never be caught.

**Fixed by hand**: each gated import moved into the file's `mod tests` (`use std::fs;`,
`use crate::worktree_sizes::directory_size_bytes_best_effort;`, `use …::CloseSessionDirtyFile;`), so the
first `#[cfg(test)]` is the test module again. The tree compiled before and after.

## What would close it

When the tidy finds an import that only a `#[cfg(test)]` module of the same file uses, it moves the
import into that module rather than gating it in the header. Where the users are several test children,
or out-of-line ones, it collects the gated imports at the end of the file instead. Separately, the gate's
counter could stop only at `#[cfg(test)]` directly followed by `mod`, but every record's history is
measured with the current rule, so that change needs its own decision.
