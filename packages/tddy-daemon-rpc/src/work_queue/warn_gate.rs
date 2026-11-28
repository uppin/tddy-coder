//! [`WarnGate`]: a warning about a key is logged when its reason first appears or changes, not on
//! every poll. `QueryBranch` is polled every few seconds per rendered branch, and a branch whose PR
//! status cannot be looked up logged the same line ~2,200 times an hour (81 MB of daemon log).

/// Remembers the last reason warned about per key.
#[derive(Debug, Default)]
pub(crate) struct WarnGate {
    // TODO(red): state for the green phase.
}

impl WarnGate {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether `reason` for `key` should be logged now: true the first time, false while the same
    /// reason repeats, true again when the reason changes or after [`Self::clear`].
    pub(crate) fn should_warn(&self, _key: &str, _reason: &str) -> bool {
        todo!("green: WarnGate::should_warn")
    }

    /// The condition behind `key` is gone (the lookup succeeded), so its next failure warns again.
    pub(crate) fn clear(&self, _key: &str) {
        todo!("green: WarnGate::clear")
    }
}
