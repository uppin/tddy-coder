//! `CreateSessionWatch`, `ListSessionWatches`, `RenewSessionWatch` and `CancelSessionWatch`
//! answered on this host (`#activity-wake` 2/3).
//!
//! The tools ([`tddy_tool_engine::project_home_watch_tools`]) keep this host's session watches,
//! `tddy_session_activity::session_watches`, for the caller's project and OS user. Before a watch is
//! created the caller's agent must be one a turn can be sent to, and every session it names must be
//! one it can see.

use tddy_tool_engine::project_home_watch_tools::ProjectHomeWatchPort;

use super::project_home::DaemonProjectHome;

// TODO(session-watches): implement — every method; the defaults refuse.
impl ProjectHomeWatchPort for DaemonProjectHome {}
