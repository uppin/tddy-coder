//! `DescribeSessionStartOptions` answered on this host (`#start-parity` 5/9).
//!
//! Each list is read from the RPC the New-session form fills its picker from, called in-process on
//! this host: the same handler, the same code path, with no peer routing (the tool forwards a call
//! that names another host to that host whole). The two stack pickers are the sessions this host
//! lists, filtered as the form filters them ([`tddy_tool_engine::start_options`]).
//!
//! A list whose source this host does not have, or that its source refused, is empty and says why in
//! its `*_unavailable` field. Any other failure of a source fails the answer.

use tddy_discovery::catalog_service::CatalogHandler;
use tddy_host_service::HostServiceImpl;
use tddy_projects::ProjectHandler;
use tddy_rpc::Request;
use tddy_service::proto::accounts::ProviderRouteOption;
use tddy_service::proto::catalog::{
    ListAgentModelsRequest, ListAgentsRequest, ListSubagentsRequest,
};
use tddy_service::proto::host::{HostService, ListSshConfigHostsRequest, ProbeOutcome};
use tddy_service::proto::project::ListProjectBranchesRequest;
use tddy_service::proto::session::ListSessionsRequest;
use tddy_session_lifecycle::connection_service::launch_ports::LaunchSessions;
use tddy_tool_engine::start_options::{
    models_agent, pr_stack_orchestrators, provider_harness, stack_base_session_candidates,
    CandidateSession, LabelledChoice, ModelChoices, ProviderRouteChoice, StartOptions,
    StartOptionsQuery,
};

use super::project_home::status_error;
use crate::families::ProviderAccountsPort;
use crate::{CatalogRpcHandler, ProjectRpcHandler};

/// Why a host with no credential vaults has no provider accounts to answer for.
const NO_PROVIDER_ACCOUNTS: &str =
    "this host keeps no credential vaults, so it has no provider accounts to offer";

/// The sources one answer is read from, on this host, for one caller.
pub(super) struct StartOptionsSources<'a> {
    pub(super) session_token: &'a str,
    pub(super) daemon_instance_id: &'a str,
    pub(super) launch: &'a LaunchSessions,
    pub(super) catalog: &'a CatalogRpcHandler,
    pub(super) projects: &'a ProjectRpcHandler,
    pub(super) host_service: &'a HostServiceImpl,
    pub(super) provider_accounts: Option<&'a dyn ProviderAccountsPort>,
}

/// The host's choices for a start `query` narrows, for `project_id`.
pub(super) async fn describe_start_options(
    sources: StartOptionsSources<'_>,
    project_id: &str,
    query: StartOptionsQuery,
) -> anyhow::Result<StartOptions> {
    let agents = agent_choices(sources.catalog).await?;
    let subagents = subagent_names(sources.catalog).await?;
    let (models, models_unavailable) = model_choices(&sources, &query).await?;
    let (provider_routes, provider_routes_unavailable) =
        provider_route_choices(&sources, &query).await;
    let (ssh_config_hosts, ssh_config_hosts_unavailable) =
        ssh_config_hosts(sources.host_service, sources.session_token).await?;
    let branches = branch_names(sources.projects, sources.session_token, project_id).await?;
    let sessions = session_candidates(&sources).await?;
    Ok(StartOptions {
        daemon_instance_id: sources.daemon_instance_id.to_string(),
        agents,
        models,
        models_unavailable,
        provider_routes,
        provider_routes_unavailable,
        ssh_config_hosts,
        ssh_config_hosts_unavailable,
        subagents,
        branches,
        stack_parents: pr_stack_orchestrators(&sessions),
        pr_stack_base_candidates: stack_base_session_candidates(
            &sessions,
            project_id,
            sources.daemon_instance_id,
        ),
    })
}

/// The agents this host's catalogue offers, as the agent picker lists them.
async fn agent_choices(catalog: &CatalogRpcHandler) -> anyhow::Result<Vec<LabelledChoice>> {
    let listed = catalog
        .list_agents(Request::direct(ListAgentsRequest {}))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed
        .agents
        .into_iter()
        .map(|agent| LabelledChoice {
            id: agent.id,
            label: agent.label,
        })
        .collect())
}

/// The subagents this host resolves, each qualified by this host as `name@daemon_instance_id`.
async fn subagent_names(catalog: &CatalogRpcHandler) -> anyhow::Result<Vec<String>> {
    let listed = catalog
        .list_subagents(Request::direct(ListSubagentsRequest {}))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed
        .subagents
        .into_iter()
        .map(|subagent| subagent.agent_id)
        .collect())
}

/// The models the query names: one provider account's, or one agent's from the vendor catalogue.
/// Its reason when the query names neither, or when the account's models cannot be listed.
async fn model_choices(
    sources: &StartOptionsSources<'_>,
    query: &StartOptionsQuery,
) -> anyhow::Result<(ModelChoices, String)> {
    let account_id = query.provider_account_id.trim();
    if !account_id.is_empty() {
        return Ok(account_models(sources, account_id).await);
    }
    let agent = match models_agent(query) {
        Ok(agent) => agent,
        Err(reason) => return Ok((ModelChoices::default(), reason)),
    };
    let listed = sources
        .catalog
        .list_agent_models(Request::direct(ListAgentModelsRequest {
            session_token: sources.session_token.to_string(),
            agent,
            daemon_instance_id: String::new(),
        }))
        .await
        .map_err(status_error)?
        .into_inner();
    let models = ModelChoices {
        default: listed.default_model,
        choices: listed
            .models
            .into_iter()
            .map(|model| LabelledChoice {
                id: model.id,
                label: model.label,
            })
            .collect(),
    };
    Ok((models, String::new()))
}

/// One provider account's models, the first preselected as the form preselects them.
async fn account_models(
    sources: &StartOptionsSources<'_>,
    account_id: &str,
) -> (ModelChoices, String) {
    let Some(port) = sources.provider_accounts else {
        return (ModelChoices::default(), NO_PROVIDER_ACCOUNTS.to_string());
    };
    match port
        .provider_account_models(sources.session_token, account_id)
        .await
    {
        Ok(ids) => {
            let default = ids.first().cloned().unwrap_or_default();
            let choices = ids
                .into_iter()
                .map(|id| LabelledChoice {
                    label: id.clone(),
                    id,
                })
                .collect();
            (ModelChoices { default, choices }, String::new())
        }
        Err(status) => (ModelChoices::default(), status.message().to_string()),
    }
}

/// The accounts that can run the query's harness, with the reason when none can be asked.
async fn provider_route_choices(
    sources: &StartOptionsSources<'_>,
    query: &StartOptionsQuery,
) -> (Vec<ProviderRouteChoice>, String) {
    let harness = match provider_harness(query) {
        Ok(harness) => harness,
        Err(reason) => return (Vec::new(), reason),
    };
    let Some(port) = sources.provider_accounts else {
        return (Vec::new(), NO_PROVIDER_ACCOUNTS.to_string());
    };
    match port.provider_routes(sources.session_token, &harness).await {
        Ok(options) => (
            options.into_iter().map(route_choice).collect(),
            String::new(),
        ),
        Err(status) => (Vec::new(), status.message().to_string()),
    }
}

fn route_choice(option: ProviderRouteOption) -> ProviderRouteChoice {
    ProviderRouteChoice {
        account_id: option.account_id,
        label: option.label,
        preset: option.preset,
        supported: option.supported,
        reason: option.reason,
    }
}

/// The SSH config hosts this host lists, or the reason it could not list them.
async fn ssh_config_hosts(
    host_service: &HostServiceImpl,
    session_token: &str,
) -> anyhow::Result<(Vec<String>, String)> {
    let answer = HostService::list_ssh_config_hosts(
        host_service,
        Request::direct(ListSshConfigHostsRequest {
            session_token: session_token.to_string(),
            daemon_instance_id: String::new(),
        }),
    )
    .await
    .map_err(status_error)?
    .into_inner();
    if answer.outcome == ProbeOutcome::Ok as i32 {
        return Ok((
            answer.hosts.into_iter().map(|host| host.alias).collect(),
            String::new(),
        ));
    }
    let reason = if answer.failure_reason.is_empty() {
        "this host could not list its SSH config".to_string()
    } else {
        answer.failure_reason
    };
    Ok((Vec::new(), reason))
}

/// The branches of `project_id`'s checkout on this host.
async fn branch_names(
    projects: &ProjectRpcHandler,
    session_token: &str,
    project_id: &str,
) -> anyhow::Result<Vec<String>> {
    let listed = projects
        .list_project_branches(Request::direct(ListProjectBranchesRequest {
            session_token: session_token.to_string(),
            project_id: project_id.to_string(),
            daemon_instance_id: String::new(),
        }))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed.branches)
}

/// Every session this host lists, as the stack pickers filter them. Each is this host's, so each
/// carries this host's id.
async fn session_candidates(
    sources: &StartOptionsSources<'_>,
) -> anyhow::Result<Vec<CandidateSession>> {
    let listed = sources
        .launch
        .list_sessions_at_session_coordinate(Request::direct(ListSessionsRequest {
            session_token: sources.session_token.to_string(),
        }))
        .await
        .map_err(status_error)?
        .into_inner();
    Ok(listed
        .sessions
        .into_iter()
        .map(|session| CandidateSession {
            session_id: session.session_id,
            project_id: session.project_id,
            daemon_instance_id: sources.daemon_instance_id.to_string(),
            branch: session.branch,
            recipe: session.recipe,
            orchestrator_session_id: session.orchestrator_session_id,
        })
        .collect())
}
