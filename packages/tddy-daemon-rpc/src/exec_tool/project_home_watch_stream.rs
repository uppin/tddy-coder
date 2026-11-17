//! `WaitSessionWatchEvents` answered on this host (`#activity-wake` 4/5).
//!
//! The tool ([`tddy_tool_engine::project_home_watch_stream_tools`]) streams the caller's session
//! watches, `tddy_session_activity::session_watches`, for its project and OS user: each call
//! acknowledges what the stream printed, renews its lease and answers the next batch — at once when
//! one waits, otherwise held until the notification bus records something for the caller's project
//! or the call's timeout passes, as `WaitProjectNotifications` is.

use async_trait::async_trait;
use serde_json::Value;
use tddy_tool_engine::project_home_watch_stream_tools::{
    ProjectHomeWatchStreamPort, WatchStreamRequest,
};
use tddy_worktree_service::session_target::HomeCaller;

use super::project_home::DaemonProjectHome;

#[async_trait]
impl ProjectHomeWatchStreamPort for DaemonProjectHome {
    async fn wait_watch_events(
        &self,
        home: &HomeCaller,
        request: WatchStreamRequest,
    ) -> Result<Value, String> {
        // TODO(watch-monitor): implement — take_stream_batch for the caller's owner; answer at once
        // when it has something, otherwise wait on the log for a record of the owner's project, or
        // the timeout, renewing the lease while the call is held.
        let _ = (home, request);
        Err(
            "TODO(watch-monitor): WaitSessionWatchEvents is not implemented on this host yet"
                .to_string(),
        )
    }
}
