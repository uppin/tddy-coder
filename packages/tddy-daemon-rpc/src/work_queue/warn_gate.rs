//! [`WarnGate`]: a warning about a key is logged when its reason first appears or changes, not on
//! every poll. `QueryBranch` is polled every few seconds per rendered branch, and a branch whose PR
//! status cannot be looked up logged the same line ~2,200 times an hour (81 MB of daemon log).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Remembers the last reason warned about per key.
#[derive(Debug, Default)]
pub(crate) struct WarnGate {
    last_reason_by_key: Mutex<HashMap<String, String>>,
}

impl WarnGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The process-wide gate, shared by every handler instance: a branch is polled by one
    /// `QueryBranch` route whichever call answers it, so the de-duplication must outlive any single
    /// handler's lifetime.
    pub(crate) fn shared() -> &'static WarnGate {
        static SHARED: OnceLock<WarnGate> = OnceLock::new();
        SHARED.get_or_init(Self::new)
    }

    /// Whether `reason` for `key` should be logged now: true the first time, false while the same
    /// reason repeats, true again when the reason changes or after [`Self::clear`].
    pub(crate) fn should_warn(&self, key: &str, reason: &str) -> bool {
        let mut last_reason_by_key = self
            .last_reason_by_key
            .lock()
            .expect("WarnGate lock poisoned");
        match last_reason_by_key.get(key) {
            Some(last) if last == reason => false,
            _ => {
                last_reason_by_key.insert(key.to_string(), reason.to_string());
                true
            }
        }
    }

    /// The condition behind `key` is gone (the lookup succeeded), so its next failure warns again.
    pub(crate) fn clear(&self, key: &str) {
        self.last_reason_by_key
            .lock()
            .expect("WarnGate lock poisoned")
            .remove(key);
    }
}
