//! An orchestrator's requests for access to paths beyond its reach, held for the operator
//! (`#orchestra` 6/9).
//!
//! `RequestPathAccess` records a request here and waits on it. The request is announced as an
//! attention notification — a pending question of kind [`PATH_ACCESS_QUESTION_KIND`] — so the
//! drawer, the desktop and Telegram tell the operator; the orchestrator's session pane lists the
//! session's requests and answers one. Allowing writes a [`PathGrant`] into the session's grant
//! store (`crate::path_grants`), so the read or write that follows is inside its reach. A request
//! nobody answers within its wait times out as denied.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::path_grants::PathGrantMode;

/// The pending-question kind an access request is announced as.
pub const PATH_ACCESS_QUESTION_KIND: &str = "path_access";

/// How long `RequestPathAccess` waits for the operator before answering denied.
pub const PATH_ACCESS_WAIT: Duration = Duration::from_secs(600);

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

/// Every session's pending access requests on this host. Cheap to clone; clones share the requests.
#[derive(Debug, Clone, Default)]
pub struct PathAccessRequests {
    // TODO(access-elicitation): the pending requests and their waiters.
}

impl PathAccessRequests {
    pub fn new() -> Self {
        Self::default()
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
        // TODO(access-elicitation): implement
        let _ = (session_dir, session_id, paths, mode, reason);
        unimplemented!("PathAccessRequests::request")
    }

    /// The session's waiting requests, oldest first.
    pub fn pending(&self, session_id: &str) -> Vec<PendingPathAccess> {
        // TODO(access-elicitation): implement
        let _ = session_id;
        unimplemented!("PathAccessRequests::pending")
    }

    /// Answer one of the session's requests. Allowing writes a grant per path, then wakes the
    /// waiter; denying only wakes it.
    pub fn answer(
        &self,
        session_id: &str,
        request_id: &str,
        allow: bool,
    ) -> Result<PathAccessOutcome, PathAccessAnswerError> {
        // TODO(access-elicitation): implement
        let _ = (session_id, request_id, allow);
        unimplemented!("PathAccessRequests::answer")
    }

    /// Wait for a request's answer, at most `wait`: no answer by then is [`PathAccessOutcome::TimedOut`]
    /// and the request is withdrawn.
    pub async fn wait(&self, request_id: &str, wait: Duration) -> PathAccessOutcome {
        // TODO(access-elicitation): implement
        let _ = (request_id, wait);
        unimplemented!("PathAccessRequests::wait")
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
