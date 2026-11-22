//! An orchestrator's requests for access to paths beyond its reach, held for the operator
//! (`#orchestra` 6/9).
//!
//! `RequestPathAccess` records a request here and waits on it. The request is announced as an
//! attention notification — a pending question of kind [`PATH_ACCESS_QUESTION_KIND`] — so the
//! drawer, the desktop and Telegram tell the operator; the orchestrator's session pane lists the
//! session's requests and answers one. Allowing writes a [`PathGrant`] into the session's grant
//! store (`crate::path_grants`), so the read or write that follows is inside its reach. A request
//! nobody answers within its wait times out as denied.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::oneshot;

use crate::path_grants::{PathGrant, PathGrantMode, PathGrantStore};

/// The pending-question kind an access request is announced as.
pub const PATH_ACCESS_QUESTION_KIND: &str = "path_access";

/// How long `RequestPathAccess` waits for the operator before answering denied.
pub const PATH_ACCESS_WAIT: Duration = Duration::from_secs(600);

/// The name a grant mode goes by on the wire and in the operator's copy: `read` or `write`.
pub fn mode_name(mode: PathGrantMode) -> &'static str {
    match mode {
        PathGrantMode::Read => "read",
        PathGrantMode::Write => "write",
    }
}

/// One request waiting for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPathAccess {
    pub request_id: String,
    pub session_id: String,
    pub paths: Vec<PathBuf>,
    pub mode: PathGrantMode,
    pub reason: String,
    pub requested_at_unix_ms: u64,
}

impl PendingPathAccess {
    /// The request as `ListPathAccessRequests` carries it.
    pub fn into_proto(self) -> tddy_service::proto::activity::PathAccessRequest {
        tddy_service::proto::activity::PathAccessRequest {
            request_id: self.request_id,
            paths: self.paths.iter().map(|p| p.display().to_string()).collect(),
            mode: mode_name(self.mode).to_string(),
            reason: self.reason,
            requested_at_unix_ms: self.requested_at_unix_ms,
        }
    }
}

/// How a request ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathAccessOutcome {
    Allowed,
    Denied,
    TimedOut,
}

/// Why an answer was not taken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAccessAnswerError {
    /// No request of that id waits for that session — answered already, timed out, or never made.
    NotPending { request_id: String },
    /// The grant could not be written; the request still waits.
    GrantNotWritten(String),
}

/// One request in flight: its record, where its grants go, and how to wake its waiter.
#[derive(Debug)]
struct Waiting {
    pending: PendingPathAccess,
    session_dir: PathBuf,
    wake: Option<oneshot::Sender<PathAccessOutcome>>,
}

impl Waiting {
    /// Write a grant per requested path into the session's grant store.
    fn grant(&self) -> Result<(), PathAccessAnswerError> {
        let store = PathGrantStore::open(&self.session_dir);
        let granted_at_unix_ms = tddy_daemon_kernel::now_unix_ms();
        for path in &self.pending.paths {
            store
                .add(PathGrant {
                    path: path.clone(),
                    mode: self.pending.mode,
                    granted_at_unix_ms,
                    reason: self.pending.reason.clone(),
                })
                .map_err(|e| PathAccessAnswerError::GrantNotWritten(e.to_string()))?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Held {
    /// Oldest first.
    waiting: Vec<Waiting>,
    /// The receiving end of each request's wake-up, taken by [`PathAccessRequests::wait`].
    waiters: HashMap<String, oneshot::Receiver<PathAccessOutcome>>,
}

/// Every session's pending access requests on this host. Cheap to clone; clones share the requests.
#[derive(Debug, Clone, Default)]
pub struct PathAccessRequests {
    held: Arc<Mutex<Held>>,
}

impl PathAccessRequests {
    pub fn new() -> Self {
        Self::default()
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        // A poisoned lock only means another thread panicked mid-update; the data stays usable.
        self.held.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record a request of the session whose directory is `session_dir`, returning it with its new
    /// id. Every path must be absolute.
    pub fn request(
        &self,
        session_dir: &Path,
        session_id: &str,
        paths: Vec<PathBuf>,
        mode: PathGrantMode,
        reason: String,
    ) -> Result<PendingPathAccess, String> {
        if paths.is_empty() {
            return Err("a request names at least one path".to_string());
        }
        if let Some(relative) = paths.iter().find(|p| !p.is_absolute()) {
            return Err(format!(
                "{} is not an absolute path; name every path in full",
                relative.display()
            ));
        }
        let pending = PendingPathAccess {
            request_id: format!("access-{}", uuid::Uuid::new_v4()),
            session_id: session_id.to_string(),
            paths,
            mode,
            reason,
            requested_at_unix_ms: tddy_daemon_kernel::now_unix_ms(),
        };
        let (wake, waiter) = oneshot::channel();
        let mut held = self.held();
        held.waiters.insert(pending.request_id.clone(), waiter);
        held.waiting.push(Waiting {
            pending: pending.clone(),
            session_dir: session_dir.to_path_buf(),
            wake: Some(wake),
        });
        Ok(pending)
    }

    /// The session's waiting requests, oldest first.
    pub fn pending(&self, session_id: &str) -> Vec<PendingPathAccess> {
        self.held()
            .waiting
            .iter()
            .filter(|w| w.pending.session_id == session_id)
            .map(|w| w.pending.clone())
            .collect()
    }

    /// Answer one of the session's requests. Allowing writes a grant per path, then wakes the
    /// waiter; denying only wakes it.
    pub fn answer(
        &self,
        session_id: &str,
        request_id: &str,
        allow: bool,
    ) -> Result<PathAccessOutcome, PathAccessAnswerError> {
        let mut held = self.held();
        let index = held
            .waiting
            .iter()
            .position(|w| w.pending.session_id == session_id && w.pending.request_id == request_id)
            .ok_or_else(|| PathAccessAnswerError::NotPending {
                request_id: request_id.to_string(),
            })?;
        let outcome = if allow {
            held.waiting[index].grant()?;
            PathAccessOutcome::Allowed
        } else {
            PathAccessOutcome::Denied
        };
        let mut waiting = held.waiting.remove(index);
        if let Some(wake) = waiting.wake.take() {
            // A waiter that already gave up has dropped its end; the grant stands regardless.
            let _ = wake.send(outcome);
        }
        Ok(outcome)
    }

    /// Withdraw a request nobody will wait for any longer (its caller went away). A no-op for a
    /// request that was answered or withdrawn already.
    pub fn withdraw(&self, request_id: &str) {
        let mut held = self.held();
        held.waiting.retain(|w| w.pending.request_id != request_id);
        held.waiters.remove(request_id);
    }

    /// Wait for a request's answer, at most `wait`: no answer by then is [`PathAccessOutcome::TimedOut`]
    /// and the request is withdrawn.
    pub async fn wait(&self, request_id: &str, wait: Duration) -> PathAccessOutcome {
        let Some(mut waiter) = self.held().waiters.remove(request_id) else {
            return PathAccessOutcome::TimedOut;
        };
        match tokio::time::timeout(wait, &mut waiter).await {
            Ok(Ok(outcome)) => outcome,
            // The request was dropped without an answer.
            Ok(Err(_)) => PathAccessOutcome::TimedOut,
            Err(_elapsed) => {
                self.held()
                    .waiting
                    .retain(|w| w.pending.request_id != request_id);
                // An answer that landed between the deadline and the withdrawal still counts.
                waiter.try_recv().unwrap_or(PathAccessOutcome::TimedOut)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_grants::PathGrantStore;

    const SESSION: &str = "01a1fdd8-0000-7000-8000-00000000000a";

    fn a_request(requests: &PathAccessRequests, session_dir: &Path) -> PendingPathAccess {
        requests
            .request(
                session_dir,
                SESSION,
                vec![PathBuf::from("/srv/shared")],
                PathGrantMode::Read,
                "read the fixtures".to_string(),
            )
            .expect("a request")
    }

    #[test]
    fn a_request_is_pending_until_answered() {
        let session = tempfile::tempdir().unwrap();
        let requests = PathAccessRequests::new();
        let made = a_request(&requests, session.path());

        assert_eq!(requests.pending(SESSION), vec![made.clone()]);
        requests.answer(SESSION, &made.request_id, false).unwrap();
        assert_eq!(requests.pending(SESSION), Vec::new());
    }

    #[test]
    fn allowing_writes_a_grant_for_each_path() {
        let session = tempfile::tempdir().unwrap();
        let requests = PathAccessRequests::new();
        let made = a_request(&requests, session.path());

        requests.answer(SESSION, &made.request_id, true).unwrap();

        let granted: Vec<PathBuf> = PathGrantStore::open(session.path())
            .grants()
            .unwrap()
            .into_iter()
            .map(|g| g.path)
            .collect();
        assert_eq!(granted, vec![PathBuf::from("/srv/shared")]);
    }

    #[test]
    fn a_relative_path_is_refused() {
        let session = tempfile::tempdir().unwrap();
        let requests = PathAccessRequests::new();

        let refused = requests.request(
            session.path(),
            SESSION,
            vec![PathBuf::from("shared")],
            PathGrantMode::Read,
            "read".to_string(),
        );

        assert!(refused.is_err(), "{refused:?}");
    }

    #[test]
    fn answering_a_request_that_is_not_pending_is_refused() {
        let requests = PathAccessRequests::new();

        let refused = requests.answer(SESSION, "access-unknown", true);

        assert_eq!(
            refused,
            Err(PathAccessAnswerError::NotPending {
                request_id: "access-unknown".to_string()
            })
        );
    }

    #[tokio::test]
    async fn an_unanswered_request_times_out_and_is_withdrawn() {
        let session = tempfile::tempdir().unwrap();
        let requests = PathAccessRequests::new();
        let made = a_request(&requests, session.path());

        let outcome = requests
            .wait(&made.request_id, Duration::from_millis(50))
            .await;

        assert_eq!(outcome, PathAccessOutcome::TimedOut);
        assert_eq!(requests.pending(SESSION), Vec::new());
    }

    #[tokio::test]
    async fn a_waiter_wakes_with_the_answer() {
        let session = tempfile::tempdir().unwrap();
        let requests = PathAccessRequests::new();
        let made = a_request(&requests, session.path());
        let waiter = {
            let requests = requests.clone();
            let id = made.request_id.clone();
            tokio::spawn(async move { requests.wait(&id, Duration::from_secs(10)).await })
        };

        requests.answer(SESSION, &made.request_id, true).unwrap();

        assert_eq!(waiter.await.unwrap(), PathAccessOutcome::Allowed);
    }
}
