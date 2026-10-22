use std::sync::Arc;

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
    ///
    /// Recording a session again (a relaunch under the same id) waits for a reach already in
    /// flight and keeps its gate, so the replaced record is not written to by that reach afterwards
    /// and a reach to the new one still queues behind it.
    pub async fn expose_terminal_to_livekit(
        &self,
        session_id: &str,
        metadata: tddy_core::session_participant_metadata::SessionParticipantMetadata,
    ) {
        let flight = self.exposure_gate(session_id).await.unwrap_or_default();
        let _in_flight = flight.lock().await;
        self.exposed_terminals.write().await.insert(
            session_id.to_string(),
            ExposedTerminalRecord {
                metadata,
                exposure: None,
                flight: Arc::clone(&flight),
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
    /// Reaches to one session are single-flight: the session's own gate is held across the
    /// exposure. Two clients connecting to one session at the same moment would otherwise each
    /// find no exposure and each start one under the same identity — and LiveKit resolves a
    /// duplicate identity by disconnecting the participant that was already there, so the second
    /// would evict the first the moment it arrived. The registry's lock is **not** held across it:
    /// a join is a token mint and a room connection, and holding the registry through one would
    /// make every other session's join, and every other registry access, wait for it.
    pub async fn ensure_exposed_terminal(
        &self,
        session_id: &str,
        at: &tddy_daemon_extensions::TerminalAddress,
        exposure: &dyn tddy_daemon_extensions::TerminalExposure,
    ) -> Result<bool, tddy_rpc::Status> {
        let Some(flight) = self.exposure_gate(session_id).await else {
            return Ok(false);
        };
        let _in_flight = flight.lock().await;
        // Looked at again under the gate: the reach ahead of this one may have exposed it, or the
        // session may have ended while this one waited.
        let metadata_json = {
            let recorded = self.exposed_terminals.read().await;
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
            Some(tddy_core::session_participant_metadata::session_metadata_json(&terminal.metadata))
        };
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
        // Written back only to the record this reach was gated by: a record removed while the join
        // ran and recorded again since belongs to another PTY, and an exposure of the old one would
        // stand in for it as "still running".
        if let Some(terminal) = self
            .exposed_terminals
            .write()
            .await
            .get_mut(session_id)
            .filter(|terminal| Arc::ptr_eq(&terminal.flight, &flight))
        {
            terminal.exposure = Some(exposed);
        }
        Ok(true)
    }

    /// The gate that serialises reaches to `session_id`, or `None` when it recorded no terminal.
    async fn exposure_gate(&self, session_id: &str) -> Option<Arc<tokio::sync::Mutex<()>>> {
        let recorded = self.exposed_terminals.read().await;
        recorded
            .get(session_id)
            .map(|terminal| Arc::clone(&terminal.flight))
    }
}
