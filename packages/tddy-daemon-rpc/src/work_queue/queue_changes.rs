//! Hearing that a work queue changed, whoever changed it.
//!
//! Every write to a queue — an RPC of this daemon, an agent's tool, a schedule firing, a writer in
//! another process — ends in one atomic rename of the queue's record
//! ([`SessionWorkQueues::update`](tddy_core::session_work_queue::SessionWorkQueues::update)), and an
//! update that changes nothing writes nothing. So the record's directory is watched: an event naming
//! the record means "read it again". A reader compares the revision it reads with the one it last
//! sent, so a burst of events, or an event for a write it already read, costs one read and sends
//! nothing new.

use std::ffi::OsString;
use std::path::Path;

use notify::{RecursiveMode, Watcher};
use tddy_rpc::Status;
use tokio::sync::mpsc;

/// A watch on one queue record. Dropping it stops the watch.
pub(crate) struct QueueChanges {
    _watcher: notify::RecommendedWatcher,
    rx: mpsc::UnboundedReceiver<()>,
}

impl QueueChanges {
    /// Watch the queue stored at `record`. Started before the record is first read, so a write
    /// landing during that read is heard.
    pub(crate) fn of_record(record: &Path) -> Result<Self, Status> {
        let dir = record.parent().ok_or_else(|| {
            Status::internal(format!("{} has no directory to watch", record.display()))
        })?;
        let name: OsString = record
            .file_name()
            .ok_or_else(|| Status::internal(format!("{} names no file", record.display())))?
            .to_os_string();
        let (tx, rx) = mpsc::unbounded_channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let names_the_record = match &event {
                    Ok(event) => event
                        .paths
                        .iter()
                        .any(|path| path.file_name() == Some(name.as_os_str())),
                    // The watcher could not say what happened (e.g. events were dropped): the record
                    // may have changed, so read it again.
                    Err(_) => true,
                };
                if names_the_record {
                    let _ = tx.send(());
                }
            })
            .map_err(|e| Status::internal(format!("watching {}: {e}", dir.display())))?;
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(|e| Status::internal(format!("watching {}: {e}", dir.display())))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Wait until the record may have changed; every event already waiting is taken with it.
    /// `None` once the watch has stopped.
    pub(crate) async fn changed(&mut self) -> Option<()> {
        self.rx.recv().await?;
        while self.rx.try_recv().is_ok() {}
        Some(())
    }
}
