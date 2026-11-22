//! The trunk watch's source, served from the inbox's branch-commits row.

use std::sync::Arc;

use async_trait::async_trait;
use tddy_github::trunk_feed::{Conditional, RepoSlug, TrunkCommit, TrunkFeed, TrunkReadError};

use crate::GithubInbox;

/// A [`TrunkFeed`] that reads `branch_commits` through the inbox, so every trunk watcher of a
/// repository shares one cached, conditional row.
pub struct InboxTrunkFeed {
    inbox: Arc<GithubInbox>,
}

impl InboxTrunkFeed {
    pub fn new(inbox: Arc<GithubInbox>) -> Self {
        Self { inbox }
    }
}

#[async_trait]
impl TrunkFeed for InboxTrunkFeed {
    async fn newest_commits(
        &self,
        repo: &RepoSlug,
        branch: &str,
        etag: Option<&str>,
    ) -> Result<Conditional<Vec<TrunkCommit>>, TrunkReadError> {
        // TODO(n2 github-event-inbox): implement — answer NotModified when the row's ETag equals
        // `etag`, else Modified with the row.
        let _ = (&self.inbox, repo, branch, etag);
        todo!("TODO(n2): implement InboxTrunkFeed::newest_commits")
    }
}
