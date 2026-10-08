use crate::agent_roster;

use tddy_daemon_livekit::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use std::path::PathBuf;

use crate::agent_host_callbacks::AgentRoster;

impl AgentRoster {
    /// Report — once per `(agents dir, name)` per process — that a registry assistant is shadowing
    /// a `<tddyhome>/agents` def of the same name.
    ///
    /// `create_assistant` refuses a name a def already answers to, so this can only happen the
    /// other way round: the def was written *after* the assistant existed. Resolution deliberately
    /// does **not** refuse in that case. `resolvable_agent_defs` answers `ListAgents`,
    /// `ListSubagents`, `StartSession` and roster attach, so making a name tie fatal here would let
    /// one stray YAML file break every agent listing and every session start on the daemon —
    /// the operator's typo would cost them the daemon. Flipping the winner instead would silently
    /// change which agent an existing session runs, which is the thing the create-time guard exists
    /// to prevent, in the other direction.
    ///
    /// So the ordering stands and the silence is what gets fixed. Deduplicated because this runs on
    /// every `ListAgents`; an undeduplicated line here would flood the log rather than inform it.
    pub(crate) fn report_shadowed_agent_def(agents_dir: &std::path::Path, name: &str) {
        pub(crate) static REPORTED: std::sync::OnceLock<
            std::sync::Mutex<std::collections::HashSet<String>>,
        > = std::sync::OnceLock::new();
        let key = format!("{}\u{0}{name}", agents_dir.display());
        let mut reported = REPORTED
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !reported.insert(key) {
            return;
        }
        log::error!(
            "agent '{name}' is defined both as a registry assistant and by a def in {} — the \
             assistant wins and the def will not resolve. Rename one of them; \
             `--agent {name}` currently runs the assistant.",
            agents_dir.display()
        );
    }

    /// Every agent def a name can resolve against on this daemon: the YAML defs
    /// under `<tddyhome>/agents`, and this daemon's registry assistants — `{builtin, yaml,
    /// sqlite}`. A registry assistant of the same name as a YAML def wins, on the same
    /// "the more specific source is the one the operator just edited" rule that already makes a
    /// YAML def beat a builtin.
    pub async fn resolvable_agent_defs(
        &self,
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        resolvable_agent_defs(&self.tddy_data_dir, self.model_registry.as_deref()).await
    }

    /// The def a session started as `agent` must actually be built from, for `caller`.
    ///
    /// Differs from [`Self::resolvable_agent_defs`] in one way that matters: a registry assistant's
    /// def comes back carrying its provider's credential. The listing path deliberately does not —
    /// `ListAgents` is answered for every operator, and a key has no business in it — but a session
    /// started without one comes up "successfully" and 401s on every model call.
    pub async fn agent_def_for_spawn(
        &self,
        agent: &str,
        caller: &str,
    ) -> Result<Option<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        let model_registry = &self.model_registry;
        let state = self.state();
        spawn_agent_def::agent_def_for_spawn(agent, caller, model_registry, state).await
    }

    /// Resolve the `specialized_agents` references that name **this** daemon against
    /// [`Self::resolvable_agent_defs`], into their full defs (see
    /// docs/ft/coder/specialized-subagents.md). Each entry is either a qualified
    /// `name@daemon_instance_id` or a bare name read as this daemon's (see [`started_agent_id`]).
    /// An unresolvable reference *of this daemon's* is a request error — the session is never
    /// started with a silently-dropped subagent. An empty input resolves to an empty output, not an
    /// error.
    ///
    /// A reference naming a **peer** resolves to no def here, and is skipped rather than refused: a
    /// def describes an agent on one host, and this list exists to build the
    /// `TDDY_SUBAGENT`/`TDDY_SUBAGENTS_JSON` jail env, which can only carry defs this host holds.
    /// That is not a silent drop — a peer-owned agent is recorded on the session's roster by
    /// [`Self::seeded_roster_records`] and reaches the main agent through the live roster
    /// `tddy-tools` reads, exactly as an agent attached after the start does
    /// (docs/ft/daemon/session-agent-roster.md § Remote agents).
    pub async fn resolve_specialized_agent_defs(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        if specialized_agents.is_empty() {
            return Ok(Vec::new());
        }
        let local_instance_id = local_instance_id_for_config(&self.config);
        let resolved = self.resolvable_agent_defs().await?;
        let mut selected = Vec::with_capacity(specialized_agents.len());
        for reference in specialized_agents {
            let id = agent_roster::started_agent_id(reference, &local_instance_id)?;
            if id.daemon_instance_id != local_instance_id {
                continue;
            }
            let def = resolved.iter().find(|d| d.name == id.name).ok_or_else(|| {
                Status::invalid_argument(format!(
                    "specialized_agents: unknown subagent '{reference}' (not found under \
                     <tddyhome>/agents, and not an assistant in this daemon's registry)"
                ))
            })?;
            selected.push(def.clone());
        }
        Ok(selected)
    }

    /// The roster a session's `specialized_agents` seed resolves to, before anything has been
    /// started for it.
    ///
    /// Resolved into **records** rather than defs, and by the same resolver an attach uses
    /// ([`Self::roster_record_for`]): an agent is placeable on any host, so what a seed names is an
    /// entry carrying a placement (`daemon_instance_id`) and a withdrawal (`replaces`), which a def
    /// — describing an agent on one host — cannot express. A reference naming a peer is answered
    /// from that peer's own `ListSubagents`, never from a local def of the same name, which is a
    /// different agent.
    ///
    /// A bare name is read as this daemon's (see [`started_agent_id`]). A reference that resolves
    /// to nothing fails the whole seed with `INVALID_ARGUMENT` naming it: a session started with a
    /// silently-dropped agent keeps the tools that agent was meant to take away and says nothing.
    /// An empty seed resolves to an empty roster, not an error.
    ///
    /// The records name no clone yet — `codebase_session_id` is filled in by
    /// [`Self::seed_session_agent_roster`], which is the only place that knows which session they
    /// are being recorded on.
    pub async fn seeded_roster_records(
        &self,
        specialized_agents: &[String],
    ) -> Result<Vec<tddy_core::SessionAgentRecord>, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let mut records = Vec::with_capacity(specialized_agents.len());
        for reference in specialized_agents {
            let id = agent_roster::started_agent_id(reference, &local_instance_id)?;
            records.push(self.roster_record_for(&id, reference).await?);
        }
        Ok(records)
    }

    /// The session directory a roster call addresses, resolved **only after** its caller has been.
    ///
    /// Auth first is load-bearing rather than tidy: attaching an agent owned by another daemon
    /// contacts that peer and provisions a checkout on it, so a check that ran afterwards would let
    /// an unauthenticated caller build a clone on another host (PRD AC12).
    pub fn roster_session_dir(
        &self,
        session_token: &str,
        session_id: &str,
    ) -> Result<PathBuf, Status> {
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        // Resolved for the authorization decision alone: a caller who maps to no OS user may not
        // reach a session, but the path itself is this daemon's, not that user's — config is the
        // single source of the sessions base (`sessions_base_for_user`).
        self.config
            .os_user_for_github(&github_user)
            .ok_or_else(|| Status::permission_denied("user not mapped to OS user"))?;
        session_dir_lookup::session_dir_for(&self.tddy_data_dir, session_id)
    }

    // ── Remote agents: room admission, clones, tool split ────────────────────────────────────
    //
    // docs/ft/daemon/session-agent-roster.md § Remote agents, § Clones.
}

use crate::session_dir_lookup;
use crate::spawn_agent_def;

/// [`DaemonSessionHost::resolvable_agent_defs`] over the two fields it reads: the YAML defs under
/// `<tddy_data_dir>/agents` and `model_registry`'s assistants, the registry winning a name tie.
///
/// Free rather than a method so `tddy-daemon-rpc`'s `ListSubagents` answers from the very list a
/// session start and a roster attach resolve against here, without holding the host — one
/// resolver, so what a picker is offered and what it can attach cannot drift apart.
pub async fn resolvable_agent_defs(
    tddy_data_dir: &std::path::Path,
    model_registry: Option<&tddy_model_registry::ModelRegistryStore>,
) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
    let agents_dir = tddy_data_dir.join("agents");
    let mut defs = tddy_discovery::agent_def::resolve_agent_defs(&agents_dir);
    for def in registry_agent_defs(model_registry).await? {
        match defs.iter_mut().find(|d| d.name == def.name) {
            Some(existing) => {
                AgentRoster::report_shadowed_agent_def(&agents_dir, &def.name);
                *existing = def;
            }
            None => defs.push(def),
        }
    }
    Ok(defs)
}

/// This daemon's registry assistants as agent defs. Empty when no registry is wired (a test
/// fixture); a registry that is wired but unreadable is an error, never "no assistants" — a
/// session started against a name that silently stopped resolving runs as something else.
async fn registry_agent_defs(
    model_registry: Option<&tddy_model_registry::ModelRegistryStore>,
) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
    match model_registry {
        Some(registry) => tddy_model_registry::registry_agent_defs(registry)
            .await
            .map_err(Status::from),
        None => Ok(Vec::new()),
    }
}
