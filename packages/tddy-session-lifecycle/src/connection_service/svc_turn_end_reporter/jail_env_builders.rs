use super::DaemonSessionHost;

use tddy_daemon_kernel::daemon_identity::local_instance_id_for_config;
use tddy_rpc::Status;

impl DaemonSessionHost {
    /// Build the `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env pair for already-resolved
    /// specialized-agent defs (see [`Self::resolve_specialized_agent_defs`]). Empty input produces
    /// no env pairs.
    ///
    /// TODO(session-agent-roster): the in-jail runner derives `--allowedTools` /
    /// `--disallowedTools` from these seeded defs, so a def whose `replaces` was edited between the
    /// attach and the relaunch changes what the relaunched main agent may call — the one thing
    /// snapshotting `replaces` into the roster exists to prevent. Closing it means handing the
    /// runner the roster's replaced set outright instead of letting it re-derive one
    /// (docs/ft/daemon/session-agent-roster.md AC25).
    pub(crate) fn specialized_subagent_env(
        &self,
        defs: &[tddy_discovery::agent_def::SpecializedAgentDef],
    ) -> Result<Vec<(String, String)>, Status> {
        if defs.is_empty() {
            return Ok(Vec::new());
        }
        let names = defs
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let defs_json = serde_json::to_string(defs).map_err(|e| {
            Status::internal(format!("failed to serialize specialized agent defs: {e}"))
        })?;
        Ok(vec![
            ("TDDY_SUBAGENT".to_string(), names),
            ("TDDY_SUBAGENTS_JSON".to_string(), defs_json),
        ])
    }

    /// Tell the jail which daemon facilitates it.
    ///
    /// Exported unconditionally, and not folded into [`Self::specialized_subagent_env`] which is
    /// skipped for a session that starts with no agents: the roster is mutated while the session
    /// runs, so a jail started empty still needs to be able to qualify what it is later told.
    ///
    /// Without it the in-jail `tddy-tools` cannot qualify its seeded agent ids — a seed resolved on
    /// this daemon is `explorer`, and the id the main agent must type is `explorer@{this daemon}`.
    /// The two other transports carry the id in their own environment
    /// (`TDDY_REMOTE_DAEMON_INSTANCE_ID`, the HTTP daemon's own); a sandbox-IPC jail is told nothing
    /// at all, and a bare id resolves against whichever daemon happens to answer.
    ///
    /// Named after the daemon's own `TDDY_DAEMON_INSTANCE_ID` startup override, so the value and the
    /// variable an operator would set to change it are spelled the same on both sides.
    pub(crate) fn jail_daemon_identity_env(&self) -> Vec<(String, String)> {
        vec![(
            "TDDY_DAEMON_INSTANCE_ID".to_string(),
            local_instance_id_for_config(&self.config),
        )]
    }

    /// The `TDDY_LSP_TOOLS` jail env pair — set when a language server is available for the
    /// session's worktree, so the in-jail `tddy-tools --mcp` exposes the `Lsp*` tools.
    pub(crate) fn lsp_tools_env(&self, worktree_root: &std::path::Path) -> Vec<(String, String)> {
        let available = tddy_core::toolcall::lsp::lsp_executor()
            .map(|ex| ex.is_available(worktree_root))
            .unwrap_or(false);
        if available {
            vec![("TDDY_LSP_TOOLS".to_string(), "rust".to_string())]
        } else {
            Vec::new()
        }
    }
}
