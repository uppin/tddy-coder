use super::CliSessionManager;

use super::livekit_bridge;

use super::LiveKitTerminalAddress;

use super::BridgedTerminal;

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
    /// ([`Self::ensure_livekit_terminal`]). A session start that reached a LiveKit server cost
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
        self.livekit_terminals.write().await.insert(
            session_id.to_string(),
            BridgedTerminal {
                metadata,
                participant: None,
            },
        );
    }

    /// Put a participant serving `session_id`'s terminal into `at`, unless one is already there or
    /// there is nothing to bridge.
    ///
    /// Returns whether the session's terminal is drivable over LiveKit once this call is done.
    /// `false` is not a failure: it means there was nothing to expose — a session type that is
    /// never reached over LiveKit, or one whose agent has exited and left no PTY behind.
    ///
    /// The write lock is held across the join deliberately, and that is what makes this
    /// single-flight. Two clients connecting to one session at the same moment would otherwise each
    /// find no participant and each connect one under the same identity — and LiveKit resolves a
    /// duplicate identity by disconnecting the participant that was already there, so the second
    /// would evict the first the moment it arrived.
    pub async fn ensure_livekit_terminal(
        &self,
        session_id: &str,
        at: &LiveKitTerminalAddress,
    ) -> anyhow::Result<bool> {
        let mut exposed = self.livekit_terminals.write().await;
        let Some(terminal) = exposed.get(session_id) else {
            return Ok(false);
        };
        if terminal
            .participant
            .as_ref()
            .is_some_and(|serving| !serving.is_finished())
        {
            return Ok(true);
        }
        let metadata = terminal.metadata.clone();
        // A session whose agent has exited keeps its block until the cleanup task removes it, so
        // the PTY is asked for rather than assumed: a bridge built on a handle that is gone would
        // serve a terminal nothing writes to.
        let Some(handle) = self.get(session_id).await else {
            return Ok(false);
        };
        let participant = livekit_bridge::spawn_livekit_bridge(
            handle,
            &at.url,
            &at.room,
            &at.api_key,
            &at.api_secret,
            &at.identity,
            Some(metadata),
        )
        .await?;
        if let Some(terminal) = exposed.get_mut(session_id) {
            terminal.participant = Some(participant);
        }
        Ok(true)
    }
}
