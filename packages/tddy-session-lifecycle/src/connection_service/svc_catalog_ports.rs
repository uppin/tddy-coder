//! Family A catalogue RPCs — host side of [`tddy_discovery::catalog_service::CatalogHandler`].

use async_trait::async_trait;
use tddy_rpc::{Request, Response, Status};
use tddy_service::proto::catalog::{
    AgentInfo as CatalogAgentInfo, ListAgentModelsRequest, ListAgentModelsResponse,
    ListAgentsRequest, ListAgentsResponse, ListSubagentsRequest, ListSubagentsResponse,
    ListToolsRequest, ListToolsResponse, SubagentInfo as CatalogSubagentInfo,
    ToolInfo as CatalogToolInfo,
};

use super::family_proto_bridge::wire_same;
use super::{
    agent_models_cache, list_models_probe_args, parse_agent_models_json, DaemonSessionHost,
    AGENT_MODELS_CACHE_TTL,
};
use crate::agent_list_mapping::agent_allowlist_rows;
use crate::connection_service::agent_roster;
use crate::livekit_peer_discovery::local_instance_id_for_config;
use tddy_spawn::spawner;

#[async_trait]
impl tddy_discovery::catalog_service::CatalogHandler for DaemonSessionHost {
    async fn list_tools(
        &self,
        _request: Request<ListToolsRequest>,
    ) -> Result<Response<ListToolsResponse>, Status> {
        self.record_rpc_activity();
        let tools: Vec<CatalogToolInfo> = self
            .config
            .allowed_tools()
            .iter()
            .map(|t| {
                let label = t
                    .label
                    .as_deref()
                    .and_then(tddy_daemon_kernel::trim_to_option)
                    .unwrap_or_else(|| t.path.clone());
                CatalogToolInfo {
                    path: t.path.clone(),
                    label,
                }
            })
            .collect();
        Ok(Response::new(ListToolsResponse { tools }))
    }

    async fn list_agents(
        &self,
        _request: Request<ListAgentsRequest>,
    ) -> Result<Response<ListAgentsResponse>, Status> {
        log::debug!("list_agents RPC: mapping config allowlist to AgentInfo");
        // A registry this daemon has but cannot read is an error, not "there are no assistants" —
        // a session started against a missing agent id fails much later and much less clearly.
        let assistants = match &self.model_registry {
            Some(registry) => registry
                .list_assistants()
                .await
                .map_err(tddy_rpc::Status::from)?,
            None => Vec::new(),
        };
        let agents: Vec<CatalogAgentInfo> = agent_allowlist_rows(&self.config, &assistants)
            .into_iter()
            .map(|row| CatalogAgentInfo {
                id: row.id,
                label: row.display_label,
            })
            .collect();
        log::info!("list_agents RPC: returning {} agent(s)", agents.len());
        Ok(Response::new(ListAgentsResponse { agents }))
    }

    /// Enumerate the models an agent supports by shelling out to `tddy-tools list-models` as the
    /// caller's OS user. Results are cached per (agent, daemon) for a short TTL. A failed probe is
    /// surfaced as an RPC error — never masked with a fallback catalog.
    ///
    /// Runs the probe on the local daemon; `daemon_instance_id` participates only in the cache key
    /// (cross-daemon forwarding is not wired here — the web fetches models from the daemon it is
    /// already connected to).
    async fn list_agent_models(
        &self,
        request: Request<ListAgentModelsRequest>,
    ) -> Result<Response<ListAgentModelsResponse>, Status> {
        let req = request.into_inner();
        let github_user = (self.user_resolver)(&req.session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?
            .to_string();

        let agent = req.agent.trim().to_string();
        if agent.is_empty() {
            return Err(Status::invalid_argument("agent is required"));
        }

        // Key by OS user: cursor / ACP catalogs (and the "current, default" model) are
        // account-specific, so one user's list must never be served to another from the cache.
        let cache_key = format!(
            "{}\u{1f}{}\u{1f}{}",
            os_user,
            req.daemon_instance_id.trim(),
            agent
        );
        if let Ok(cache) = agent_models_cache().lock() {
            if let Some((cached_at, resp)) = cache.get(&cache_key) {
                if cached_at.elapsed() < AGENT_MODELS_CACHE_TTL {
                    return Ok(Response::new(resp.clone()));
                }
            }
        }

        let tools_path = self.resolve_tddy_tools_path();
        // Cursor's model probe must hand tddy-tools the resolved absolute `agent` path (as the PTY
        // spawn does), so the impersonated child execs a fully-qualified binary instead of doing a
        // PATH lookup that lacks the install dir. Only forward an absolute path — a bare-name
        // resolution keeps the existing behavior (no `--cursor-cli-path`).
        let cursor_cli_path = (agent == "cursor")
            .then(|| crate::config::resolve_cursor_binary_path(&self.config))
            .filter(|p| std::path::Path::new(p).is_absolute())
            .map(std::path::PathBuf::from);
        let probe_args = list_models_probe_args(&agent, cursor_cli_path.as_deref());
        let probe = tokio::task::spawn_blocking(move || {
            spawner::run_capture_as_user(&os_user, &tools_path, &probe_args)
        })
        .await
        .map_err(|e| Status::internal(format!("model probe join error: {e}")))?
        .map_err(|e| Status::failed_precondition(format!("model probe failed: {e}")))?;

        let resp = parse_agent_models_json(&probe)?;

        if let Ok(mut cache) = agent_models_cache().lock() {
            cache.insert(cache_key, (std::time::Instant::now(), resp.clone()));
        }
        Ok(Response::new(resp))
    }

    /// Resolved specialized-agent defs available to wire into a managed-codebase session — every
    /// source a name can resolve against here, so `<tddyhome>/agents/*.yaml` (see
    /// docs/ft/coder/specialized-subagents.md) *and* this daemon's registry assistants.
    ///
    /// Answered from [`Self::resolvable_agent_defs`], which is also what an attach resolves the id
    /// it is handed against: what a picker is offered and what it can then attach are one list, not
    /// two that can drift. Advertising less than that is what made an assistant created in Models &
    /// Agents invisible to the roster while being perfectly attachable by name.
    ///
    /// Every row is stamped with this daemon's instance id and the qualified `agent_id` it is
    /// attached by. A picker fans this call out across every common-room daemon, and two of them
    /// routinely answer with a def called `explorer`: without the stamp the merged list cannot say
    /// which host offers which row, and the id the picker sends would be a guess rather than the
    /// one the serving daemon minted.
    ///
    /// A def whose own name contains `@` is dropped with a warning: its qualified id would parse
    /// back as a different pair, so advertising it would hand a picker an id that routes elsewhere.
    async fn list_subagents(
        &self,
        _request: Request<ListSubagentsRequest>,
    ) -> Result<Response<ListSubagentsResponse>, Status> {
        log::debug!("list_subagents RPC: resolving agent defs");
        let daemon_instance_id = local_instance_id_for_config(&self.config);
        let defs = self.resolvable_agent_defs().await?;
        let resolved = defs.len();
        let subagents: Vec<CatalogSubagentInfo> = defs
            .into_iter()
            .filter_map(
                |def| match agent_roster::subagent_info(&def, &daemon_instance_id) {
                    Ok(info) => wire_same(&info).ok(),
                    Err(e) => {
                        log::warn!("list_subagents RPC: not advertising a def — {e}");
                        None
                    }
                },
            )
            .collect();
        // An empty answer has three very different causes — this daemon has no defs, its registry
        // was never wired in, or a def was dropped on the way out — and "returning 0" told them
        // apart in none of them. Naming the sources is what makes an empty picker diagnosable from
        // the log alone, on a host whose filesystem is not to hand.
        log::info!(
            "list_subagents RPC: returning {} subagent(s) of {} resolved def(s) [agents dir {}, \
             model registry {}]: {:?}",
            subagents.len(),
            resolved,
            self.tddy_data_dir.join("agents").display(),
            if self.model_registry.is_some() {
                "attached"
            } else {
                "absent"
            },
            subagents.iter().map(|s| &s.agent_id).collect::<Vec<_>>()
        );
        Ok(Response::new(ListSubagentsResponse { subagents }))
    }

    // ── Session agent roster (docs/ft/daemon/session-agent-roster.md) ─────────────────────────
}
