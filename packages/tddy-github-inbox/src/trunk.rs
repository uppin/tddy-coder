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
        let row = self
            .inbox
            .branch_commits(repo, branch)
            .await
            .map_err(|e| TrunkReadError(format!("{e:?}")))?;
        // The row is revalidated by the inbox; a caller already holding this ETag has nothing new.
        if etag.is_some() && etag == row.etag.as_deref() {
            return Ok(Conditional::NotModified {
                poll_interval_ms: None,
            });
        }
        Ok(Conditional::Modified {
            value: row.value,
            etag: row.etag,
            poll_interval_ms: None,
        })
    }
}
