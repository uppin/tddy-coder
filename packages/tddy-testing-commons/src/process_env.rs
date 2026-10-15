//! Serialized, self-restoring mutation of the process environment.
//!
//! The environment is process-wide, so two tests of one binary that set or read the same variable
//! race. [`ProcessEnv::lock`] takes one lock for the whole binary; the variables a test changes
//! through it are put back, and the lock released, when it is dropped. A test that needs to see a
//! variable unchanged, or a child that must not inherit a half-set state, takes the same lock.

use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Holds the process-environment lock and the prior values of every variable changed through it.
#[must_use = "the environment is restored and the lock released when this is dropped"]
pub struct ProcessEnv {
    saved: Vec<(String, Option<OsString>)>,
    // Declared last so the variables are restored before the lock is released.
    _lock: MutexGuard<'static, ()>,
}

impl ProcessEnv {
    /// Waits for the lock. A lock poisoned by a test that panicked is taken anyway: the panicking
    /// test's own guard restored the variables while unwinding.
    pub fn lock() -> Self {
        let lock = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self {
            saved: Vec::new(),
            _lock: lock,
        }
    }

    /// Sets `name` to `value` until this guard is dropped.
    pub fn setting(mut self, name: &str, value: &str) -> Self {
        self.remember(name);
        std::env::set_var(name, value);
        self
    }

    /// Removes `name` until this guard is dropped.
    pub fn without(mut self, name: &str) -> Self {
        self.remember(name);
        std::env::remove_var(name);
        self
    }

    fn remember(&mut self, name: &str) {
        self.saved.push((name.to_string(), std::env::var_os(name)));
    }
}

impl Drop for ProcessEnv {
    fn drop(&mut self) {
        for (name, prior) in self.saved.drain(..).rev() {
            match prior {
                Some(value) => std::env::set_var(&name, value),
                None => std::env::remove_var(&name),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VARIABLE: &str = "TDDY_PROCESS_ENV_GUARD_TEST";
    /// Held by the one test that must set it before taking the lock.
    const PRIOR_VARIABLE: &str = "TDDY_PROCESS_ENV_GUARD_PRIOR_TEST";

    #[test]
    fn a_variable_set_through_the_guard_is_gone_again_once_it_is_dropped() {
        // Given a variable set through the guard
        let env = ProcessEnv::lock().setting(VARIABLE, "set");
        assert_eq!(std::env::var(VARIABLE).as_deref(), Ok("set"));

        // When the guard is dropped
        drop(env);

        // Then the variable is absent again
        assert!(std::env::var_os(VARIABLE).is_none());
    }

    #[test]
    fn a_variable_removed_through_the_guard_is_given_back_its_prior_value() {
        // Given a variable, no other test touches, holding a value, then removed through a guard
        std::env::set_var(PRIOR_VARIABLE, "prior");
        let env = ProcessEnv::lock().without(PRIOR_VARIABLE);
        assert!(std::env::var_os(PRIOR_VARIABLE).is_none());

        // When the guard is dropped
        drop(env);

        // Then the value it held is back
        assert_eq!(std::env::var(PRIOR_VARIABLE).as_deref(), Ok("prior"));
        std::env::remove_var(PRIOR_VARIABLE);
    }

    #[test]
    fn a_lock_poisoned_by_a_panicking_test_is_still_taken() {
        // Given a test that panicked while holding the lock
        let _ = std::thread::spawn(|| {
            let _env = ProcessEnv::lock().setting(VARIABLE, "poisoning");
            panic!("poison the lock");
        })
        .join();

        // When another test takes the lock
        let env = ProcessEnv::lock();

        // Then it gets it, and the panicking test's variable was restored while unwinding
        assert!(std::env::var_os(VARIABLE).is_none());
        drop(env);
    }
}
