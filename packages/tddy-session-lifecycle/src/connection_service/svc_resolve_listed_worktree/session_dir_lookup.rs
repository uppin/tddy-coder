use super::DaemonSessionHost;

use tddy_core::output::SESSIONS_SUBDIR;

use tddy_core::session_lifecycle::validate_session_id_segment;

use tddy_rpc::Status;

use std::path::PathBuf;

impl DaemonSessionHost {
    /// Where a session this daemon serves keeps its `.session.yaml`.
    ///
    /// The id is validated as a single path segment before it is joined, because every roster call
    /// takes it from the caller and the directory it names is read-modify-written: an id carrying
    /// `../` would have an attach rewrite another user's `.session.yaml` outside this daemon's
    /// sessions base entirely.
    pub(crate) fn session_dir_for(&self, session_id: &str) -> Result<PathBuf, Status> {
        validate_session_id_segment(session_id)
            .map_err(|e| Status::invalid_argument(e.message()))?;
        Ok(self.tddy_data_dir.join(SESSIONS_SUBDIR).join(session_id))
    }
}
