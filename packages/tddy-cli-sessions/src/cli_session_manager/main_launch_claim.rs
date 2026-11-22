//! The **main-launch claim**: one (re)launch of a session's `main` agent at a time.
//!
//! A resume of a CLI harness session spends a long time before its agent is registered — over a
//! Coder target, re-wiring and starting the agent there takes minutes. Nothing registered under
//! `main` in that window tells a second resume that one is already on its way, so without a claim
//! both launch: the second registration replaces the first in the terminal index, and the first
//! agent runs on with nothing able to stop it. A resume therefore claims the session's main launch
//! before it does anything else, and holds the claim until it has finished, either way.
//!
//! The claim is taken **synchronously**, before the claiming future first awaits: two resumes
//! polled together (`tokio::join!`, two RPCs on one task) are decided by which is polled first,
//! never by which finishes a lookup first.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::CliSessionManager;

/// The sessions whose `main` is being (re)launched right now.
pub(super) type MainLaunches = Arc<Mutex<HashSet<String>>>;

/// Why [`CliSessionManager::claim_main_launch`] refused a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainLaunchRefusal {
    /// Another launch of this session's `main` holds the claim and has not finished.
    Launching,
    /// The session's `main` is registered: its agent is running, so there is nothing to relaunch
    /// until it is stopped.
    Running,
}

/// A held claim on one session's `main` launch. The claim is released when this is dropped, so a
/// launch that fails — or whose future is dropped half-way — never leaves the session unlaunchable.
#[must_use = "the claim is released as soon as it is dropped"]
#[derive(Debug)]
pub struct MainLaunchClaim {
    launches: MainLaunches,
    session_id: String,
}

impl Drop for MainLaunchClaim {
    fn drop(&mut self) {
        lock(&self.launches).remove(&self.session_id);
    }
}

impl CliSessionManager {
    /// Claim the (re)launch of `session_id`'s `main` agent, for as long as the returned
    /// [`MainLaunchClaim`] is held.
    ///
    /// Refused with [`MainLaunchRefusal::Launching`] while another claim on the session is held,
    /// and with [`MainLaunchRefusal::Running`] when the session's `main` is already registered. A
    /// launch that succeeds registers its `main` before its claim is dropped, so the next claim is
    /// refused as running until that agent ends.
    pub async fn claim_main_launch(
        &self,
        session_id: &str,
    ) -> Result<MainLaunchClaim, MainLaunchRefusal> {
        // Taken before the first await: see the module documentation.
        if !lock(&self.main_launches).insert(session_id.to_string()) {
            return Err(MainLaunchRefusal::Launching);
        }
        let claim = MainLaunchClaim {
            launches: self.main_launches.clone(),
            session_id: session_id.to_string(),
        };
        if self.get(session_id).await.is_some() {
            return Err(MainLaunchRefusal::Running);
        }
        Ok(claim)
    }
}

/// The set is only ever inserted into and removed from, so a panic while it was held cannot have
/// left it half-written: a poisoned lock is still a consistent set.
fn lock(launches: &MainLaunches) -> std::sync::MutexGuard<'_, HashSet<String>> {
    launches
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
