use super::CliSessionManager;

use super::pty_terminal_service::pty_terminal_service;

use super::ExposedTerminalRecord;

impl CliSessionManager {
    /// Attach a session's [`ManagedWorkflow`](crate::session_toolcall::ManagedWorkflow) so its
    /// toolcall listener + controller stay alive for the session's lifetime. Dropped when the
    /// session's main terminal exits (see `spawn_terminal_cleanup`).
    pub async fn attach_managed_workflow(
        &self,
        session_id: &str,
        managed: crate::session_toolcall::ManagedWorkflow,
    ) {
        self.managed_workflows
            .write()
            .await
            .insert(session_id.to_string(), managed);
    }

    /// Record what `session_id` publishes about itself when its terminal is bridged into LiveKit.
    ///
    /// Local and instant, which is the point: a session start says what the session *is*, and
    /// joining a room on its behalf is deferred to the first LiveKit consumer
    /// ([`Self::ensure_exposed_terminal`]). A session start that reached a LiveKit server cost
    /// whatever reaching it cost — nothing when the server answered, the caller's whole patience
    /// when it was configured and down.
    ///
    /// Calling this is what makes a session drivable from another host at all. A session type that
    /// is served over the daemon's own connection and never over LiveKit — `cursor-cli`,
    /// `workspace` — records nothing here and is bridged nowhere.
    pub async fn expose_terminal_to_livekit(
        &self,
        session_id: &str,
        metadata: tddy_core::session_participant_metadata::SessionParticipantMetadata,
    ) {
        self.exposed_terminals.write().await.insert(
            session_id.to_string(),
            ExposedTerminalRecord {
                metadata,
                exposure: None,
            },
        );
    }

    /// Expose `session_id`'s recorded terminal through `exposure` at `at`, unless an exposure is
    /// still running or there is nothing to expose.
    ///
    /// `Ok(false)` is not a failure: the session recorded no terminal, or its PTY is gone. A
    /// `FailedPrecondition` from the exposure — nothing wired to expose through — is the same
    /// answer and is returned as `Ok(false)` for the same reason; every other refusal is an error.
    ///
    /// The write lock is held across the exposure deliberately, and that is what makes this
    /// single-flight. Two clients connecting to one session at the same moment would otherwise each
    /// find no exposure and each start one under the same identity — and LiveKit resolves a
    /// duplicate identity by disconnecting the participant that was already there, so the second
    /// would evict the first the moment it arrived.
    pub async fn ensure_exposed_terminal(
        &self,
        session_id: &str,
        at: &tddy_daemon_extensions::TerminalAddress,
        exposure: &dyn tddy_daemon_extensions::TerminalExposure,
    ) -> Result<bool, tddy_rpc::Status> {
        let mut recorded = self.exposed_terminals.write().await;
        let Some(terminal) = recorded.get(session_id) else {
            return Ok(false);
        };
        if terminal
            .exposure
            .as_ref()
            .is_some_and(|serving| !serving.is_finished())
        {
            return Ok(true);
        }
        let metadata_json = Some(
            tddy_core::session_participant_metadata::session_metadata_json(&terminal.metadata),
        );
        // A session whose agent has exited keeps its record until the cleanup task removes it, so
        // the PTY is asked for rather than assumed: an exposure built on a handle that is gone
        // would serve a terminal nothing writes to.
        let Some(handle) = self.get(session_id).await else {
            return Ok(false);
        };
        let exposed = match exposure
            .expose(at, pty_terminal_service(handle), metadata_json)
            .await
        {
            Ok(exposed) => exposed,
            Err(refused) if refused.code == tddy_rpc::Code::FailedPrecondition => return Ok(false),
            Err(refused) => return Err(refused),
        };
        if let Some(terminal) = recorded.get_mut(session_id) {
            terminal.exposure = Some(exposed);
        }
        Ok(true)
    }
}
