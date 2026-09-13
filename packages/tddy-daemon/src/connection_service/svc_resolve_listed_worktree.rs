use tddy_core::output::SESSIONS_SUBDIR;

use tddy_core::session_lifecycle::validate_session_id_segment;

use std::path::Path;

use crate::{
    connection_service::agent_roster, project_storage, user_sessions_path::repos_base_for_user,
};
use tddy_spawn::{spawn_worker, spawner};

use crate::livekit_peer_discovery::local_instance_id_for_config;

use tddy_rpc::Status;

use std::path::PathBuf;

use super::ConnectionServiceImpl;

impl ConnectionServiceImpl {
    /// Ensure the project's working copy exists on this (local) host before a session starts,
    /// auto-cloning it when missing: from the local registry's `git_url` if the project is already
    /// registered here, otherwise from a peer daemon that hosts it (reusing the logical
    /// `project_id`). The blocking clone runs off the async runtime; a project unknown locally and
    /// on every peer surfaces as `NotFound`.
    ///
    /// When `agent_clone` is set, this daemon is the **owning** side of an agent clone and the
    /// project is provisioned from the facilitating daemon's `remote_git.RemoteGitService` rather
    /// than from a peer-discovered forge URL — `git clone {facilitating_instance_id}:{project_id}`
    /// with `GIT_SSH_COMMAND=tddy-remote-git-repo --daemon-url {facilitating_daemon_url}
    /// --session-token {session_token}`. That closes the two cases the peer fan-out fails for a
    /// remote owning daemon: a peer with no common room of its own, and a project whose `git_url`
    /// names a forge the peer cannot reach (PRD AC37).
    pub(crate) async fn ensure_project_available_for_start(
        &self,
        os_user: &str,
        projects_dir: &Path,
        project_id: &str,
        session_token: &str,
        agent_clone: Option<&tddy_service::proto::connection::AgentClonePlacement>,
    ) -> Result<project_storage::ProjectData, Status> {
        // Resolved lazily: only a peer-provisioned clone needs a base path. A locally-registered
        // project (the common case) never consults it, so a `None` here must not fail the start —
        // it only surfaces as an error if `ensure_project_available_locally` actually needs it.
        let repos_base_dir = repos_base_for_user(os_user, self.config.repos_base_path_or_default());
        let spawn_client = self.spawn_client.clone();
        let os_user_owned = os_user.to_string();
        let projects_dir_owned = projects_dir.to_path_buf();
        let project_id_owned = project_id.to_string();
        let timeout = self.config.spawn_worker_request_timeout();

        // An agent clone provisions from its facilitating daemon directly, so it never fans out
        // `ListProjects` across the common room — the fan-out is the path that fails for a peer
        // with no room of its own, and it is the one AC37 replaces.
        let facilitating = match agent_clone {
            Some(placement) => {
                let facilitating_instance_id = placement.facilitating_daemon_instance_id.trim();
                let facilitating_daemon_url = placement.facilitating_daemon_url.trim();
                if facilitating_instance_id.is_empty() || facilitating_daemon_url.is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "agent_clone placement is incomplete: facilitating_daemon_instance_id and \
                         facilitating_daemon_url are both required to provision a clone on a daemon \
                         that has never seen the project (got instance_id={facilitating_instance_id:?}, \
                         url={facilitating_daemon_url:?})"
                    )));
                }
                Some((
                    facilitating_instance_id.to_string(),
                    facilitating_daemon_url.to_string(),
                ))
            }
            None => None,
        };

        // Peer discovery is an async RPC fan-out; resolve it here rather than inside the blocking
        // clone task. It is only needed when the project is not registered on this host — a
        // locally-registered project clones from its stored `git_url` with no peer lookup — so the
        // fan-out is skipped entirely in that case. An agent clone skips the fan-out unconditionally
        // (it provisions from its facilitator, above).
        let registered_locally = project_storage::find_project(projects_dir, project_id)
            .map_err(|e| Status::internal(format!("read project registry: {e}")))?
            .is_some();
        let peer_entries = if registered_locally || facilitating.is_some() {
            Vec::new()
        } else {
            self.eligible_daemon_source
                .peer_project_entries(session_token)
                .await
        };

        let spawn_backend = tddy_spawn::supervisor_client::spawn_backend_choice(&self.config);
        // `ensure_project_available_locally` clones synchronously while the supervisor's client is
        // async. Handing the closure a runtime handle keeps that seam here: the closure already runs
        // on a blocking thread, which is precisely where awaiting a future by blocking belongs.
        let runtime = tokio::runtime::Handle::current();

        // The transport-shim env var a facilitator-provisioned clone sets on the `git clone` child.
        // `tddy-remote-git-repo` takes its daemon URL and session token as `--long` flags on the
        // `GIT_SSH_COMMAND` string, so a single env var carries both — the supervisor's
        // `allowed_env_keys` needs only `GIT_SSH_COMMAND` to admit it.
        let ssh_command = facilitating.as_ref().map(|(_, daemon_url)| {
            format!(
                "{} --daemon-url {daemon_url} --session-token {session_token}",
                crate::project_provision::resolve_remote_git_repo_path()
            )
        });
        let facilitating_remote_url = facilitating
            .as_ref()
            .map(|(instance_id, _)| format!("{instance_id}:{project_id_owned}"));

        let handle = tokio::task::spawn_blocking(move || {
            let cloner = |git_url: &str, dest: &Path| -> Result<(), String> {
                match &spawn_backend {
                    tddy_spawn::supervisor_client::SpawnBackendChoice::Supervisor {
                        socket_path,
                    } => {
                        let mut env = std::collections::BTreeMap::new();
                        if let Some(ref ssh) = ssh_command {
                            env.insert("GIT_SSH_COMMAND".to_string(), ssh.clone());
                        }
                        runtime
                            .block_on(
                                tddy_spawn::supervisor_spawn::clone_repo_via_supervisor_with_env(
                                    socket_path,
                                    &os_user_owned,
                                    git_url,
                                    dest,
                                    env,
                                ),
                            )
                            .map_err(|e| format!("{e:#}"))
                    }
                    tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker => {
                        if let Some(ref client) = spawn_client {
                            if ssh_command.is_none() {
                                // No transport env var to carry: the forked worker's `clone_repo` is
                                // the original path (it has no env-var channel).
                                return client
                                    .clone_repo(spawn_worker::CloneRequest {
                                        os_user: os_user_owned.clone(),
                                        git_url: git_url.to_string(),
                                        destination: dest.display().to_string(),
                                    })
                                    .map_err(|e| e.to_string());
                            }
                        }
                        // Facilitator clone (carries `GIT_SSH_COMMAND`) or no forked worker at all:
                        // the in-process `clone_as_user_with_env` carries the transport env var directly.
                        let extra: Vec<(&str, &str)> = ssh_command
                            .as_ref()
                            .map(|ssh| vec![("GIT_SSH_COMMAND", ssh.as_str())])
                            .unwrap_or_default();
                        spawner::clone_as_user_with_env(&os_user_owned, git_url, dest, &extra)
                            .map_err(|e| e.to_string())
                    }
                }
            };
            if let Some(remote_url) = facilitating_remote_url {
                crate::project_provision::ensure_project_available_from_facilitator(
                    &projects_dir_owned,
                    &project_id_owned,
                    repos_base_dir.as_deref(),
                    &remote_url,
                    cloner,
                )
            } else {
                let peer_lookup = |id: &str| {
                    peer_entries
                        .iter()
                        .find(|p| p.project_id == id)
                        .map(|p| (p.name.clone(), p.git_url.clone()))
                };
                crate::project_provision::ensure_project_available_locally(
                    &projects_dir_owned,
                    &project_id_owned,
                    repos_base_dir.as_deref(),
                    cloner,
                    peer_lookup,
                )
            }
        });

        match tokio::time::timeout(timeout, handle).await {
            Ok(Ok(res)) => res,
            Ok(Err(join_err)) => Err(Status::internal(join_err.to_string())),
            Err(_elapsed) => Err(Status::deadline_exceeded(
                "ensure_project_available_for_start: clone timed out",
            )),
        }
    }

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
        let agents_dir = self.tddy_data_dir.join("agents");
        let mut defs = tddy_discovery::agent_def::resolve_agent_defs(&agents_dir);
        for def in self.registry_agent_defs().await? {
            match defs.iter_mut().find(|d| d.name == def.name) {
                Some(existing) => {
                    Self::report_shadowed_agent_def(&agents_dir, &def.name);
                    *existing = def;
                }
                None => defs.push(def),
            }
        }
        Ok(defs)
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
        if let Some(registry) = &self.model_registry {
            // The registry wins over a YAML def of the same name, the same way it does in
            // `resolvable_agent_defs`.
            if let Some(def) =
                tddy_model_registry::registry_agent_def_with_credential(registry, agent, caller)
                    .await
                    .map_err(Status::from)?
            {
                return Ok(Some(def));
            }
        }
        let agents_dir = self.tddy_data_dir.join("agents");
        Ok(tddy_discovery::agent_def::resolve_agent_defs(&agents_dir)
            .into_iter()
            .find(|d| d.name == agent))
    }

    /// This daemon's registry assistants as agent defs. Empty when no registry is wired (a test
    /// fixture); a registry that is wired but unreadable is an error, never "no assistants" — a
    /// session started against a name that silently stopped resolving runs as something else.
    pub(crate) async fn registry_agent_defs(
        &self,
    ) -> Result<Vec<tddy_discovery::agent_def::SpecializedAgentDef>, Status> {
        match &self.model_registry {
            Some(registry) => tddy_model_registry::registry_agent_defs(registry)
                .await
                .map_err(Status::from),
            None => Ok(Vec::new()),
        }
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
    pub(crate) async fn resolve_specialized_agent_defs(
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
    pub(crate) async fn seeded_roster_records(
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
    pub(crate) fn roster_session_dir(
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
        self.session_dir_for(session_id)
    }

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

    // ── Remote agents: room admission, clones, tool split ────────────────────────────────────
    //
    // docs/ft/daemon/session-agent-roster.md § Remote agents, § Clones.

    /// Open the session's room over a checkout this daemon holds, unless it is open already.
    ///
    /// The one place a room is opened outside a split start, and the reason session *creation* no
    /// longer opens one: a room is what a session is reached through, so it is created when
    /// something first reaches for it. Every caller here is such a reach — a client connecting to
    /// the session, an owning daemon being admitted to it — and each of them is already waiting on
    /// a LiveKit round trip by asking.
    ///
    /// `Ok(None)` means this daemon has no LiveKit credentials at all and hosts no rooms; each
    /// caller decides what that means for it.
    pub(crate) async fn ensure_session_room(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_root: &Path,
    ) -> Result<Option<crate::session_room::OpenedSessionRoom>, Status> {
        let local_instance_id = local_instance_id_for_config(&self.config);
        let hosting = crate::session_room::DaemonRoomHosting {
            config: &self.config,
            instance_id: &local_instance_id,
            rooms: &self.session_rooms,
        }
        .for_worktree(session_id, worktree_root, session_dir);
        self.session_rooms
            .ensure_open(
                &hosting,
                tddy_service::ConnectionServiceServer::new(self.clone()),
                self,
            )
            .await
    }
}
