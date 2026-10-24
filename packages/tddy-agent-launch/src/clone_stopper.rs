//! Stopping a clone when the start that runs it is cancelled.
//!
//! The clone blocks a thread of the blocking pool, so the cancellation is watched from the async
//! runtime: it reaches the clone through the [`CloneHandle`] the backend hands back once git runs —
//! whichever backend that is (the forked worker, this process, or the supervisor).

use std::sync::{Arc, Mutex};

use tddy_spawn::clone_run::CloneHandle;
use tddy_task::CancellationToken;

/// Stops the clone it was given when `cancel` is signalled, before or after the clone started.
pub(crate) struct CloneStopper {
    cancel: CancellationToken,
    handle: Arc<Mutex<Option<CloneHandle>>>,
    watcher: tokio::task::JoinHandle<()>,
}

impl CloneStopper {
    /// Watch `cancel` on `runtime` until this stopper is dropped.
    pub(crate) fn watching(cancel: &CancellationToken, runtime: &tokio::runtime::Handle) -> Self {
        let handle: Arc<Mutex<Option<CloneHandle>>> = Arc::default();
        let watcher = runtime.spawn({
            let cancel = cancel.clone();
            let handle = Arc::clone(&handle);
            async move {
                cancel.cancelled().await;
                let _ = tokio::task::spawn_blocking(move || stop_the_clone(&handle)).await;
            }
        });
        Self {
            cancel: cancel.clone(),
            handle,
            watcher,
        }
    }

    /// The clone is running: remember how to stop it — or stop it now, if the start was cancelled
    /// before git began.
    pub(crate) fn started(&self, clone: CloneHandle) {
        *self.handle.lock().expect("clone handle lock") = Some(clone);
        if self.cancel.is_cancelled() {
            stop_the_clone(&self.handle);
        }
    }
}

impl Drop for CloneStopper {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

fn stop_the_clone(handle: &Mutex<Option<CloneHandle>>) {
    let Some(clone) = handle.lock().expect("clone handle lock").take() else {
        return;
    };
    if let Err(error) = clone.stop() {
        log::warn!(
            "could not stop the cancelled clone (pid {}): {error}",
            clone.pid()
        );
    }
}
