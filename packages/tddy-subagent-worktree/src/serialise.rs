//! One conversation's mutating git steps, one at a time.
//!
//! [`ConversationWorktrees`](crate::ConversationWorktrees) is built afresh for every call, so the
//! lock cannot live in it: it is keyed by the worktree's path in a process-wide map. Two concurrent
//! calls of one conversation would otherwise both try to create the worktree, or both stage and
//! commit in it and fight over its `index.lock`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

type Locks = Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>;

fn locks() -> &'static Locks {
    static LOCKS: OnceLock<Locks> = OnceLock::new();
    LOCKS.get_or_init(Locks::default)
}

/// Wait for the exclusive right to change the conversation worktree at `root`; held until dropped.
pub(crate) async fn exclusive(root: &Path) -> OwnedMutexGuard<()> {
    let lock = {
        // A poisoned map is still a map of locks: nothing in it is half-updated.
        let mut held = locks().lock().unwrap_or_else(|e| e.into_inner());
        Arc::clone(held.entry(root.to_path_buf()).or_default())
    };
    lock.lock_owned().await
}

// A lock is never forgotten: dropping one while a waiter holds it would let the next call build a
// second lock for the same worktree. An entry is a path and an `Arc`, one per conversation ever used.
