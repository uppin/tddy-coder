//! Single-flight: one request per row at a time.
//!
//! A reader takes the row's lock before it asks GitHub, then looks at the cache again: whoever was
//! ahead of it has already filled the row, so it answers from there and sends nothing.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

/// A table of per-key async locks; a key's entry lives only while someone holds or awaits it.
pub(crate) struct InFlight<K> {
    locks: Arc<Mutex<HashMap<K, Arc<AsyncMutex<()>>>>>,
}

impl<K: Hash + Eq + Clone> InFlight<K> {
    pub(crate) fn new() -> Self {
        Self {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Wait for `key`'s turn. Dropping the guard hands the turn on.
    pub(crate) async fn acquire(&self, key: K) -> FlightGuard<K> {
        let lock = self
            .locks
            .lock()
            .expect("in-flight table poisoned")
            .entry(key.clone())
            .or_default()
            .clone();
        FlightGuard {
            guard: Some(lock.lock_owned().await),
            key,
            locks: self.locks.clone(),
        }
    }
}

pub(crate) struct FlightGuard<K: Hash + Eq> {
    guard: Option<OwnedMutexGuard<()>>,
    key: K,
    locks: Arc<Mutex<HashMap<K, Arc<AsyncMutex<()>>>>>,
}

impl<K: Hash + Eq> Drop for FlightGuard<K> {
    fn drop(&mut self) {
        let mut locks = self.locks.lock().expect("in-flight table poisoned");
        // Release first, so the table's own handle is the only one left when nobody else waits.
        self.guard.take();
        if locks
            .get(&self.key)
            .is_some_and(|lock| Arc::strong_count(lock) == 1)
        {
            locks.remove(&self.key);
        }
    }
}
