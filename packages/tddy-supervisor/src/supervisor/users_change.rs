//! Applying a changed `users.yaml` (`#agent-tools` 31/32): rebuild the per-user daemon sockets,
//! then restart every service that serves users, so it is handed the new set.
//!
//! Kept out of `supervisor.rs`, which is at its length budget
//! (`packages/tddy-supervisor/docs/code-issues/oversized-file-supervisor.md`, closed).
//!
//! **Order.** The new listeners exist before the old daemon is stopped, and a user both mappings
//! name keeps their listener ([`crate::user_sockets::UserListeners::rebuild`]), so their clients
//! queue in the kernel across the restart instead of being refused. A restart drops every relayed
//! agent and the daemon's in-memory state; sessions resume from disk.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::signals::{signal_process, TERMINATE};
use crate::supervisor::Supervisor;
use crate::users_file::UsersFile;

/// How long a service that serves users gets to exit before its restart is given up on.
const RESTART_STOP_TIMEOUT: Duration = Duration::from_secs(10);
const RESTART_POLL_INTERVAL: Duration = Duration::from_millis(25);

impl Supervisor {
    /// Make `next` the mapping in force: rebuild the user sockets of every service declaring
    /// `serves_users`, then restart those services with the rebuilt handover.
    pub async fn apply_users_change(self: &Arc<Self>, next: &UsersFile) -> anyhow::Result<()> {
        let Some(users_config) = self.users_config().cloned() else {
            self.set_current_users(next.clone());
            return Ok(());
        };
        let serving: Vec<usize> = {
            let mut slots = self.lock();
            let mut serving = Vec::new();
            for (index, slot) in slots.iter_mut().enumerate() {
                if !slot.service.serves_users {
                    continue;
                }
                slot.user_listeners = slot.user_listeners.rebuild(
                    &slot.service.name.clone(),
                    &users_config.sockets_dir,
                    next,
                )?;
                serving.push(index);
            }
            serving
        };
        self.set_current_users(next.clone());

        for index in serving {
            let (name, pid) = {
                let slots = self.lock();
                let slot = &slots[index];
                (slot.service.name.clone(), slot.runtime.status().pid)
            };
            let Some(pid) = pid else {
                continue;
            };
            {
                let mut slots = self.lock();
                slots[index].runtime.record_stop_requested();
            }
            signal_process(pid, TERMINATE);
            let deadline = Instant::now() + RESTART_STOP_TIMEOUT;
            loop {
                self.reap().await;
                let still_running = self.lock()[index].runtime.status().pid == Some(pid);
                if !still_running {
                    break;
                }
                if Instant::now() >= deadline {
                    anyhow::bail!(
                        "service `{name}` did not exit within {RESTART_STOP_TIMEOUT:?} of a \
                         users.yaml change; it keeps the old mapping until it is restarted"
                    );
                }
                tokio::time::sleep(RESTART_POLL_INTERVAL).await;
            }
            self.start(index).await;
        }
        Ok(())
    }
}
